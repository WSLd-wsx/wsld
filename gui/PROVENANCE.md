# Shared GUI migration provenance

The shared frontend was copied byte-for-byte from the local wsx-pro checkout at
commit `6418529bece854effda9c5dbd9ebd73c85e8aac8` as part of the owner's
community/Store edition redesign. The private checkout is not a community build
or runtime dependency. Baseline SHA-256 values are:

| File | SHA-256 |
| --- | --- |
| `ui/index.html` | `67b66214f8adea9bbcdf068b77227e6d3ec49c9670b85e83ba886f825572cc87` |
| `ui/app.js` | `71a7d4c0b3e9c2e7d846b26fed059e71467faa17ef7f433b6ec48b986d6af586` |
| `ui/style.css` | `105d26fcdc536d122e450df044286e5622e4229e1cda3dd7758c89f18eb0a7cb` |

`community/src/store_transition.rs` copies the existing Windows Store listing,
package discovery and activation helper from the same checkout, SHA-256
`768379351cb7a561abc3e4f631fc5decb8bb4c1063a443da9be6fdc04d571228`.
The Store entitlement, purchase and paid-action implementation remains private.
See the current repository license and third-party notices. The owner selected
BSL 1.1 for original repository material on 2026-11-16.
