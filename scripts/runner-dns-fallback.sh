#!/usr/bin/env bash
# Supply temporary IPv4 host mappings only when the runner's resolver fails.
set -euo pipefail
hosts=(archive.ubuntu.com mirrors.edge.kernel.org codeload.github.com pkgs.dev.azure.com)
for host in "${hosts[@]}"; do
    if getent ahostsv4 "$host" >/dev/null 2>&1; then
        continue
    fi
    address=
    for resolver in 'cloudflare-dns.com:1.1.1.1' 'dns.google:8.8.8.8'; do
        name=${resolver%%:*}
        ip=${resolver#*:}
        path=dns-query
        if [[ "$name" == dns.google ]]; then path=resolve; fi
        response=$(curl --silent --show-error --fail --connect-timeout 10 --max-time 20 \
            --resolve "$name:443:$ip" -H 'accept: application/dns-json' \
            "https://$name/$path?name=$host&type=A" 2>/dev/null) || continue
        address=$(printf '%s' "$response" | python3 -c '
import ipaddress, json, sys
try:
    value = json.load(sys.stdin)
    if value.get("Status") != 0:
        raise ValueError("DNS query failed")
    addresses = [str(ipaddress.IPv4Address(x["data"])) for x in value.get("Answer", []) if x.get("type") == 1]
    if not addresses:
        raise ValueError("No IPv4 answer")
    print(addresses[0])
except (ValueError, KeyError, TypeError):
    sys.exit(1)
' 2>/dev/null) || continue
        break
    done
    if [[ -z "$address" ]]; then
        printf 'Cannot resolve required build host: %s\n' "$host" >&2
        exit 1
    fi
    printf '%s %s\n' "$address" "$host" | sudo tee -a /etc/hosts >/dev/null
    printf 'Added temporary DNS fallback for %s\n' "$host"
done
