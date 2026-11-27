const invoke = window.__TAURI__.core.invoke;
const $ = id => document.getElementById(id);
let running = false;
let polling = false;
let terminal = null;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
function log(message) { const output = $('log'); if (output) { output.textContent += `${message}\n`; output.scrollTop = output.scrollHeight; } }
function setRunning(value) { running = value; if ($('start')) $('start').disabled = value; if ($('cancel')) $('cancel').disabled = !value; }
async function start() {
  const steps = Number($('steps').value), failAt = Number($('fail').value);
  terminal = null;
  try {
    await invoke('probe_start', { steps, failAt });
    $('progress').max = steps; $('progress').value = 0;
    $('status').textContent = 'Running'; setRunning(true); log(`Started ${steps} steps`);
    return true;
  } catch (error) { $('status').textContent = String(error); log(`Error: ${error}`); return false; }
}
function initAcceptanceDiagnostics() {
  if ($('start')) return;
  const panel = document.createElement('section');
  panel.className = 'diagnostics';
  panel.innerHTML = '<label>Steps <input id="steps" type="number" value="100"></label><label>Fail at step <input id="fail" type="number" value="0"></label><div class="actions"><button id="start">Start task</button><button id="cancel" disabled>Cancel</button></div><progress id="progress" max="100" value="0"></progress><div id="status" role="status">Ready</div><pre id="log"></pre>';
  document.querySelector('main').append(panel);
  $('start').addEventListener('click', start);
  $('cancel').addEventListener('click', () => invoke('probe_cancel').catch(error => log(String(error))));
}
setInterval(async () => {
  if (!running || polling) return;
  polling = true;
  try {
    const value = await invoke('probe_poll');
    $('progress').value = value.progress;
    $('status').textContent = `${['Running','Succeeded','Cancelled','Controlled failure'][value.state]} · ${value.progress} steps`;
    if (value.state !== 0) { terminal = value.state; setRunning(false); log($('status').textContent); }
  } catch (error) { setRunning(false); log(`Poll error: ${error}`); }
  finally { polling = false; }
}, 40);
window.addEventListener('error', event => {
  log(`Frontend error contained: ${event.message}`);
  invoke('probe_note', { message: 'frontend-error-contained' }).catch(() => {});
});
window.addEventListener('unhandledrejection', event => log(`Frontend rejection: ${event.reason}`));
// Only a DLL built with the acceptance feature injects a call to this routine.
window.runAcceptance = async () => {
  initAcceptanceDiagnostics();
  const note = message => invoke('probe_note', { message });
  const waitTerminal = async expected => {
    for (let i = 0; i < 200 && terminal === null; i++) await sleep(20);
    if (terminal !== expected) throw new Error(`Expected state ${expected}, got ${terminal}`);
  };
  try {
    await note('acceptance-start');
    $('steps').value = 25; $('fail').value = 0; await start(); await waitTerminal(1);
    await note('success-visible');
    $('steps').value = 100; await start(); await sleep(180);
    if ($('progress').value === 0) throw new Error('No visible progress');
    await invoke('probe_cancel'); await waitTerminal(2); await note('cancel-visible');
    $('steps').value = 40; $('fail').value = 15; await start(); await waitTerminal(3); await note('failure-visible');
    $('steps').value = 0; $('fail').value = 0;
    if (await start()) throw new Error('Invalid input accepted');
    await note('invalid-input-visible');
    setTimeout(() => { throw new Error('Intentional frontend exception'); }, 0);
    await sleep(150);
    $('steps').value = 15; await start(); await waitTerminal(1); await note('core-survived-js-error');
    await sleep(2500);
    $('steps').value = 100; await start(); await sleep(100);
    await note('close-during-active-task');
    await invoke('probe_close');
  } catch (error) { log(`ACCEPTANCE FAILED: ${error}`); await note(`acceptance-failed: ${String(error).slice(0,100)}`); }
};

window.runCloseAcceptance = async () => {
  initAcceptanceDiagnostics();
  $('steps').value = 100; $('fail').value = 0;
  if (await start()) await invoke('probe_note', { message: 'os-close-ready' });
};

// Called only by an acceptance build to exercise the real IPC permission path.
window.runInstallProgressPermissionAcceptance = async () => {
  try {
    const progress = JSON.parse(await invoke('install_progress'));
    if (typeof progress.active !== 'boolean') throw new Error('Invalid progress response');
    await invoke('probe_note', { message: 'install-progress-permission-ok' });
  } catch (error) {
    await invoke('probe_note', { message: `install-progress-permission-failed: ${String(error).slice(0, 100)}` });
  } finally { await invoke('probe_close'); }
};

// Invoked only in an acceptance build; verifies actual IPC and identity.
window.runRoutingContextAcceptance = async () => {
  try {
    const environment = await invoke('store_environment');
    if (environment.packaged) {
      const entitlement = await invoke('store_entitlement');
      if (typeof entitlement !== 'boolean') throw new Error('Invalid Store entitlement response');
      await invoke('probe_note', { message: entitlement ? 'routing-store-pro-confirmed' : 'routing-store-free-confirmed' });
    }
    await invoke('probe_note', { message: environment.packaged ? 'routing-context-store' : 'routing-context-portable' });
  } catch (error) {
    await invoke('probe_note', { message: `routing-context-failed: ${String(error).slice(0, 100)}` });
  } finally { await invoke('probe_close'); }
};

let inventoryInFlight = null;
let inventoryLogged = false;
const launching = new Set();
const stopping = new Set();
const launchErrors = new Map();
let toastTimer = null;
let currentSnapshot = null;
let licensed = false;
let licenseKnown = false;
let isCommunity = false;
let currentCommand = null;
let installedGuid = null;
let installPending = false;
let installTaskDismissTimer = null;
function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
function renderInventory(snapshot) {
  if (snapshot.schema_version !== 1 || !Array.isArray(snapshot.observed_wsl)) throw new Error('Unsupported inventory response');
  const rows = [...snapshot.observed_wsl];
  if (rows.some(row => typeof row.registration !== 'string' || typeof row.wsl_running !== 'boolean')) throw new Error('Invalid distribution data');
  const isNew = row => installedGuid && typeof row.registration_guid === 'string' && row.registration_guid.replace(/[{}]/g, '').toLowerCase() === installedGuid.replace(/[{}]/g, '').toLowerCase();
  rows.sort((a, b) => Number(!!isNew(b)) - Number(!!isNew(a)) || Number(b.wsx_desktop === 'verified') - Number(a.wsx_desktop === 'verified') || Number(b.wsl_running) - Number(a.wsl_running) || a.registration.localeCompare(b.registration));
  currentSnapshot = snapshot;
  const list = $('inventory');
  list.replaceChildren();
  $('total-count').textContent = rows.length;
  $('running-count').textContent = rows.filter(row => row.wsl_running).length;
  $('desktop-count').textContent = rows.filter(row => row.wsx_desktop === 'verified').length;
  if (!rows.length) { list.append(element('div', 'message', 'No WSL distributions are registered.')); return; }
  for (const row of rows) {
    const identity = JSON.stringify([row.registration, row.registration_guid]);
    const verified = row.wsx_desktop === 'verified';
    const uncertain = row.wsx_desktop === 'unverified' || (row.wsx_desktop === undefined && row.wsx_owned);
    const card = element('article', 'distro');
    if (isNew(row)) {
      card.classList.add('newly-installed');
      card.setAttribute('aria-label', `${row.registration}, newly installed`);
    }
    const icon = element('div', `distro-icon${verified ? '' : ' other'}`, verified ? 'W' : 'L');
    if (isNew(row)) icon.append(element('span', 'new-badge', 'NEW'));
    icon.setAttribute('aria-hidden', 'true');
    const copy = element('div', 'distro-copy');
    copy.append(element('div', 'distro-name', row.registration));
    const meta = element('div', 'distro-meta');
    if (verified) {
      meta.append(element('span', 'badge', 'WSX DESKTOP'));
      const label = row.distribution_id === 'WSX-omarchy' ? 'Omarchy' : row.distribution_id === 'WSX-cachyos-plasma' ? 'CachyOS Plasma' : 'Linux desktop';
      meta.append(element('span', '', label));
    } else if (uncertain) {
      meta.append(element('span', 'badge uncertain', 'WSX RECORD UNVERIFIED'));
      meta.append(element('span', '', 'Desktop ownership could not be confirmed'));
    } else {
      meta.append(element('span', '', 'WSL distribution'));
    }
    copy.append(meta);
    const actions = element('div', 'card-actions');
    const terminalKey = `${row.registration}:terminal`;
    const desktopKey = `${row.registration}:desktop`;
    const stopPending = stopping.has(identity);
    const state = element('button', `state${row.wsl_running ? ' running' : ''}`);
    state.type = 'button';
    state.disabled = !row.wsl_running || stopPending || launching.has(terminalKey) || launching.has(desktopKey);
    if (row.wsl_running && !stopPending) {
      state.title = 'Stop this running distribution';
      state.setAttribute('aria-label', `Stop ${row.registration} (running)`);
    }
    const dot = element('span', 'dot');
    dot.setAttribute('aria-hidden', 'true');
    state.append(dot, document.createTextNode(stopPending ? 'Stopping…' : row.wsl_running ? 'Stop' : 'Stopped'));
    state.addEventListener('click', () => stopDistribution(row, identity));
    const terminalButton = element('button', 'launch terminal-action', launching.has(terminalKey) ? 'Opening…' : 'Open terminal');
    terminalButton.type = 'button';
    terminalButton.disabled = typeof row.registration_guid !== 'string' || launching.has(terminalKey) || stopPending;
    terminalButton.setAttribute('aria-label', `Open terminal for ${row.registration}`);
    terminalButton.addEventListener('click', () => launch(row, false, identity));
    actions.append(terminalButton);
    if (verified) {
      const desktopButton = element('button', `launch ${licensed ? 'desktop-action' : 'pro-action'}`, launching.has(desktopKey) ? 'Opening…' : 'Open Desktop');
      desktopButton.type = 'button';
      desktopButton.disabled = typeof row.registration_guid !== 'string' || launching.has(desktopKey) || stopPending;
      desktopButton.setAttribute('aria-label', `${licensed ? 'Open Linux desktop for' : 'Upgrade to open desktop for'} ${row.registration}`);
      desktopButton.addEventListener('click', () => {
        if (!licenseKnown) { showToast('Pro access is not confirmed. Use Check Pro access and try again.', true); return; }
        if (licensed) launch(row, true, identity); else showUpgrade(row.registration);
      });
      actions.append(desktopButton);
    }
    card.append(icon, copy, state, actions);
    if (launchErrors.has(identity)) {
      const explanation = element('div', 'card-warning', launchErrors.get(identity));
      explanation.setAttribute('role', 'alert');
      card.classList.add('has-warning');
      card.append(explanation);
    }
    list.append(card);
  }
}
function refreshInventory() {
  if (inventoryInFlight) return inventoryInFlight;
  $('refresh').disabled = true;
  inventoryInFlight = (async () => {
    try {
      renderInventory(JSON.parse(await invoke('inventory')));
      if (!inventoryLogged) { inventoryLogged = true; invoke('probe_note', { message: 'shared-inventory-visible' }).catch(() => {}); }
    } catch (error) {
      $('inventory').replaceChildren(element('div', 'message error', `Unable to load distributions: ${error}`));
      for (const id of ['total-count', 'running-count', 'desktop-count']) $(id).textContent = '—';
    } finally { inventoryInFlight = null; $('refresh').disabled = false; }
  })();
  return inventoryInFlight;
}
async function refreshAfterMutation() {
  // A refresh already in flight may have sampled WSL before the operation finished.
  if (inventoryInFlight) await inventoryInFlight;
  await refreshInventory();
}
function needsPowerShell(name) { return !/^[\p{L}\p{N} ._-]+$/u.test(name); }
function shellCommand(program, name, option = '-d') {
  // Core rejects control characters, slashes, leading hyphens and names over
  // 256 UTF-8 bytes. PowerShell single-quoted literals preserve metacharacters.
  if (!name || name.startsWith('-') || /[\/\\\p{Cc}]/u.test(name) || new TextEncoder().encode(name).length > 256) return null;
  return needsPowerShell(name) ? `${program} ${option} '${name.replace(/'/g, "''")}'` : `${program} ${option} "${name}"`;
}
function desktopCommand(name) { return shellCommand('wsld.exe', name); }
function showUpgrade(name) {
  currentCommand = desktopCommand(name);
  $('upgrade-description').textContent = isCommunity ? 'Get the Store version to use Pro desktop features.' : 'Upgrade to Pro to launch your desktop with one click.';
  $('upgrade-instruction').textContent = needsPowerShell(name) ? 'Or use the command line in PowerShell:' : 'Or use the command line:';
  $('upgrade-command').textContent = currentCommand || 'Run wsld.exe -d with the distro name as a quoted argument.';
  $('copy-command').disabled = !currentCommand;
  $('upgrade-status').textContent = '';
  $('upgrade-dialog').showModal();
}
function showToast(text, isError = false) {
  const toast = $('launch-message');
  clearTimeout(toastTimer);
  toast.textContent = text;
  toast.className = isError ? 'toast error' : 'toast';
  toast.hidden = false;
  toastTimer = setTimeout(() => { toast.hidden = true; }, isError ? 7000 : 4000);
}
async function launch(row, desktop, identity) {
  const key = `${row.registration}:${desktop ? 'desktop' : 'terminal'}`;
  if (launching.has(key) || stopping.has(identity)) return;
  launchErrors.delete(identity); launching.add(key); renderInventory(currentSnapshot);
  try {
    await invoke('launch_distribution', { name: row.registration, guid: row.registration_guid, desktop });
    showToast(desktop ? `Desktop request completed for ${row.registration}.` : `Terminal requested for ${row.registration}.`);
  } catch (error) {
    const detail = String(error);
    if (desktop && detail.includes('WSX Pro license required')) {
      licensed = false;
      showUpgrade(row.registration);
    }
    else {
      launchErrors.set(identity, detail.includes('Concurrent desktops require PCI Owner configuration ABI 2') ? 'Incompatible with running WSX desktops.' : `Could not open: ${detail}`);
      showToast(`Could not open ${row.registration}. See the message on its card.`, true);
    }
  } finally {
    launching.delete(key); renderInventory(currentSnapshot);
    await refreshInventory();
  }
}
function showStopCommand(name) {
  const command = shellCommand('wsld.exe', name, '-t');
  $('stop-instruction').textContent = command
    ? `Run this command in ${needsPowerShell(name) ? 'PowerShell' : 'a terminal'} to stop ${name}.`
    : 'Run wsld.exe -t with the distribution name as a quoted argument.';
  $('stop-command').textContent = command || '';
  $('stop-command').hidden = !command;
  $('stop-dialog').showModal();
}
async function stopDistribution(row, identity) {
  if (!row.wsl_running || stopping.has(identity)) return;
  if (isCommunity || !licensed) {
    showStopCommand(row.registration);
    return;
  }
  if (typeof row.registration_guid !== 'string') {
    showToast('The WSL registration could not be verified. Refresh the list and try again.', true);
    return;
  }
  launchErrors.delete(identity);
  stopping.add(identity);
  renderInventory(currentSnapshot);
  try {
    const stopped = await invoke('stop_distribution', { name: row.registration, guid: row.registration_guid });
    if (stopped) showToast(`Stopped ${row.registration}.`);
    else showStopCommand(row.registration);
  } catch (error) {
    launchErrors.set(identity, `Could not stop: ${error}`);
    showToast(`Could not stop ${row.registration}. See the message on its card.`, true);
  } finally {
    stopping.delete(identity);
    await refreshAfterMutation();
  }
}
$('stop-close').addEventListener('click', () => $('stop-dialog').close());
$('refresh').addEventListener('click', refreshInventory);
const environmentReady = invoke('store_environment');
let routingWarningShown = false;
let handoffTimer = null;
let handoffChecking = false;
async function activateStoreEdition() {
  await invoke('store_activate_edition');
  await invoke('probe_close');
}
async function beginStoreHandoff() {
  const environment = await invoke('store_environment');
  if (environment.packaged) throw new Error('Already running the Store edition');
  if (environment.installed) {
    $('upgrade-status').textContent = 'Opening the Store edition. Sign in with the Microsoft account used for your purchase, or upgrade to Pro there.';
    await activateStoreEdition();
    return;
  }
  await invoke('store_open_listing');
  const instructions = 'Install WSX Desk in Microsoft Store. We will open it when installation finishes.';
  $('upgrade-status').textContent = instructions;
  if (!$('upgrade-dialog').open) showToast(instructions);
  if (handoffTimer !== null) clearInterval(handoffTimer);
  handoffTimer = setInterval(async () => {
    if (handoffChecking) return;
    handoffChecking = true;
    try {
      const current = await invoke('store_environment');
      if (current.installed) {
        clearInterval(handoffTimer); handoffTimer = null;
        $('upgrade-status').textContent = 'Opening the Store edition. Sign in with the Microsoft account used for your purchase, or upgrade to Pro there.';
        await activateStoreEdition();
      }
    } catch (error) {
      clearInterval(handoffTimer); handoffTimer = null;
      $('upgrade-status').textContent = `Unable to open the Store edition: ${error}`;
      if (!$('upgrade-dialog').open) showToast('Unable to open the Store edition. Open it from Start.', true);
    } finally { handoffChecking = false; }
  }, 3000);
}
let checkingLicense = false;
async function checkEntitlement() {
  if (checkingLicense) return;
  checkingLicense = true;
  try {
    const environment = await environmentReady;
    isCommunity = environment.edition === 'community';
    $('edition-footer-text').textContent = isCommunity ? 'Community Core: BSL 1.1' : 'wsxdesk.com';
    $('edition-footer').hidden = false;
    $('product-version').textContent = `DESK · ${environment.version}`;
    document.body.classList.toggle('community-edition', isCommunity);
    document.body.classList.toggle('store-edition', !isCommunity);
    $('edition-label').textContent = isCommunity ? 'Community Edition' : 'MS Store Edition';
    $('store-buy').textContent = isCommunity ? 'Get Store version' : 'Upgrade to Pro';
    $('portable-store-note').hidden = !isCommunity;
    if (isCommunity) {
      licensed = false; licenseKnown = true;
      $('store-status').textContent = 'Portable free edition · Pro is available in Microsoft Store';
      $('store-restore').hidden = true; $('store-upgrade').textContent = 'Get Store version'; $('store-upgrade').hidden = false;
      if (environment.warning && !routingWarningShown) {
        routingWarningShown = true;
        showToast(`Could not open the Store edition. Continuing with the free edition: ${environment.warning}`, true);
      }
      return;
    }
    $('store-upgrade').textContent = 'Upgrade to Pro';
    licensed = await invoke('store_entitlement');
    licenseKnown = true;
    $('edition-label').textContent = licensed ? 'MS Store Pro' : 'MS Store Edition';
    $('store-status').textContent = licensed ? 'WSX Pro enabled' : 'WSX Pro not enabled';
    $('store-upgrade').hidden = licensed;
    if (licensed && $('upgrade-dialog').open) $('upgrade-dialog').close();
  } catch (error) {
    licensed = false; licenseKnown = false; $('store-upgrade').hidden = true; $('store-status').textContent = 'Unable to check Pro access';
    if ($('edition-label').textContent === 'CHECKING') $('edition-label').textContent = 'WSX Desk';
    invoke('probe_note', { message: String(error).includes('not allowed') ? 'store-check-denied' : 'store-check-failed' }).catch(() => {});
  } finally {
    checkingLicense = false;
    if (currentSnapshot) renderInventory(currentSnapshot);
  }
}
$('store-restore').addEventListener('click', checkEntitlement);
async function purchasePro(environment) {
  if (!environment.packaged) throw new Error('Start the installed Microsoft Store package to purchase Pro.');
  await invoke('store_purchase');
  await checkEntitlement();
}
let requestingPro = false;
async function requestStorePro(reportError = message => showToast(message, true)) {
  if (requestingPro) return;
  requestingPro = true;
  try {
    await purchasePro(await environmentReady);
  } catch (error) {
    reportError(`Purchase unavailable: ${error}`);
  } finally {
    requestingPro = false;
  }
}
async function handleStoreUpgrade(event) {
  const button = event.currentTarget;
  const dialogOpen = $('upgrade-dialog').open;
  const reportError = dialogOpen
    ? message => { $('upgrade-status').textContent = message; }
    : message => showToast(message, true);
  button.disabled = true;
  $('upgrade-status').textContent = '';
  try {
    const environment = await environmentReady;
    if (environment.edition === 'community') await beginStoreHandoff();
    else await requestStorePro(reportError);
  } catch (error) {
    reportError(isCommunity ? `Unable to open the Store edition: ${error}` : `Purchase unavailable: ${error}`);
  } finally {
    button.disabled = false;
  }
}
$('store-upgrade').addEventListener('click', handleStoreUpgrade);
$('store-buy').addEventListener('click', handleStoreUpgrade);
$('close-upgrade').addEventListener('click', () => $('upgrade-dialog').close());
$('copy-command').addEventListener('click', async () => {
  if (!currentCommand) return;
  try { await navigator.clipboard.writeText(currentCommand); $('upgrade-status').textContent = 'Command copied.'; }
  catch (_) {
    const field = document.createElement('textarea');
    field.value = currentCommand; field.style.position = 'fixed'; field.style.opacity = '0';
    document.body.append(field); field.select();
    let copied = false;
    try { copied = document.execCommand('copy'); } catch (_) {} finally { field.remove(); }
    $('upgrade-status').textContent = copied ? 'Command copied.' : 'Select and copy the command above.';
  }
});
refreshInventory();
checkEntitlement();
setInterval(() => { if (!document.hidden) refreshInventory(); }, 15000);
setInterval(() => { if (!document.hidden) checkEntitlement(); }, 60000);

// Invoked only by an acceptance-feature build; no entitlement is fabricated.
window.runSharedInventoryAcceptance = async () => {
  const note = message => invoke('probe_note', { message });
  try {
    for (let i = 0; i < 200 && !currentSnapshot; i++) await sleep(100);
    if (!currentSnapshot) throw new Error('Inventory did not load');
    if (document.querySelector('.diagnostics')) throw new Error('Diagnostics appeared in the product view');
    const verified = currentSnapshot.observed_wsl.find(row => row.wsx_desktop === 'verified');
    if (!verified) throw new Error('No verified WSX distribution available');
    licensed = false; renderInventory(currentSnapshot);
    const cards = [...document.querySelectorAll('.distro')];
    const card = cards.find(node => node.querySelector('.distro-name').textContent === verified.registration);
    const terminalButton = card.querySelector('.terminal-action');
    const desktopButton = card.querySelector('.pro-action');
    if (!terminalButton || terminalButton.disabled || terminalButton.title || !desktopButton || desktopButton.disabled || desktopButton.textContent !== 'Open Desktop' || desktopButton.title || card.querySelector('.card-command')) throw new Error('Free actions were not presented correctly');
    if (desktopCommand("test $env:USER's name") !== "wsld.exe -d 'test $env:USER''s name'" || desktopCommand('bad/name') !== null) throw new Error('Command escaping failed');
    desktopButton.click();
    if (!$('upgrade-dialog').open || $('upgrade-command').textContent !== desktopCommand(verified.registration)) throw new Error('Upgrade dialog was not presented correctly');
    $('upgrade-dialog').close();
    const inventoryTop = $('inventory').getBoundingClientRect().top;
    showToast('Terminal requested for acceptance test.');
    if ($('launch-message').hidden || getComputedStyle($('launch-message')).position !== 'fixed' || $('inventory').getBoundingClientRect().top !== inventoryTop) throw new Error('Toast changed inventory layout');
    await note('shared-inventory-free-actions-ok');
  } catch (error) { await note(`shared-inventory-failed: ${String(error).slice(0,100)}`); }
};

let checkingUpdates = false;
let downloadingUpdate = false;
async function checkUpdates() {
  if (checkingUpdates || downloadingUpdate) return;
  checkingUpdates = true; $('update-check').disabled = true;
  try {
    const environment = await environmentReady;
    if (environment.edition === 'community' || !environment.packaged) {
      $('update-button').hidden = true; $('update-check').hidden = true;
      $('update-status').textContent = environment.edition === 'community'
        ? 'Community Edition: update by downloading a newer ZIP.'
        : 'Start the installed Microsoft Store package to check for updates.';
      return;
    }
    const available = await invoke('store_check_updates');
    $('update-button').hidden = !available;
    $('update-status').textContent = available ? 'A new version is available.' : 'WSX is up to date.';
  } catch (error) {
    $('update-button').hidden = true;
    $('update-status').textContent = `Unable to check Microsoft Store for updates: ${error}`;
  } finally { checkingUpdates = false; $('update-check').disabled = false; }
}
$('update-check').addEventListener('click', checkUpdates);
$('update-button').addEventListener('click', async () => {
  if (downloadingUpdate) return;
  downloadingUpdate = true; $('update-button').disabled = true; $('update-check').disabled = true;
  $('update-status').textContent = 'Downloading update through Microsoft Store…';
  try {
    const downloaded = await invoke('store_download_update');
    $('update-button').hidden = true;
    $('update-status').textContent = downloaded ? 'Update downloaded. Microsoft Store manages installation.' : 'No update is currently available.';
  } catch (error) { $('update-status').textContent = `Update download did not complete: ${error}`; }
  finally { downloadingUpdate = false; $('update-button').disabled = false; $('update-check').disabled = false; }
});
checkUpdates();
setInterval(() => { if (!document.hidden) checkUpdates(); }, 3600000);

let catalogRequest = 0;
$('add-distro').addEventListener('click', async () => {
  if (!licenseKnown) { showToast('Pro access is not confirmed. Use Check Pro access and try again.', true); return; }
  if (!licensed) {
    currentCommand = 'wsld.exe -l -o';
    $('upgrade-description').textContent = isCommunity ? 'Get the Store version for one-click WSX distro installation.' : 'Upgrade to Pro to install a WSX distro with one click.';
    $('upgrade-instruction').textContent = 'Or list releases with the command below, then run wsld.exe --install ID[-VERSION]:';
    $('upgrade-command').textContent = currentCommand;
    $('copy-command').disabled = false; $('upgrade-status').textContent = '';
    $('upgrade-dialog').showModal();
    return;
  }
  $('install-content').replaceChildren(); $('install-status').textContent = '';
  $('install-dialog').showModal();
  const request = ++catalogRequest;
  const spinner = element('span', 'catalog-spinner');
  spinner.setAttribute('aria-hidden', 'true');
  $('install-status').replaceChildren(spinner, document.createTextNode('Checking stable releases…'));
  try {
    const catalog = JSON.parse(await invoke('online_releases'));
    if (request !== catalogRequest) return;
    if (catalog.schema_version !== 1 || !Array.isArray(catalog.releases) || !Array.isArray(catalog.rejected)) throw new Error('Unsupported release catalog');
    $('install-content').replaceChildren();
    if (!catalog.releases.length) $('install-content').append(element('p', '', 'No compatible stable releases are currently available.'));
    for (const release of catalog.releases) {
      if (typeof release.tag !== 'string' || typeof release.distribution_id !== 'string' || typeof release.version !== 'string') throw new Error('Invalid release');
      const row = element('div', 'release-row');
      row.append(element('span', '', `${release.distribution_id} · ${release.version}`));
      const button = element('button', '', 'Install'); button.type = 'button';
      button.addEventListener('click', async () => {
        if (installPending) return;
        installPending = true; $('install-close').disabled = true; $('add-distro').disabled = true;
        for (const b of $('install-content').querySelectorAll('button')) b.disabled = true;
        const card = $('install-task');
        clearTimeout(installTaskDismissTimer); installTaskDismissTimer = null;
        card.classList.remove('task-success', 'task-error');
        card.replaceChildren(); card.hidden = false;
        card.append(element('div', 'task-title', `Installing ${release.distribution_id} · ${release.version}`));
        const status = element('div', 'task-status', 'Preparing installation…');
        const progress = document.createElement('progress'); progress.className = 'task-progress';
        const detail = element('div', 'task-detail', 'Downloading, verifying, and importing.');
        const note = element('div', 'task-detail', 'Please keep WSX open.');
        card.append(status, progress, detail, note);
        $('install-status').textContent = '';
        $('install-dialog').close();
        let polling = false, finished = false;
        const timer = setInterval(async () => {
          if (polling) return;
          polling = true;
          try {
            const value = JSON.parse(await invoke('install_progress'));
            if (finished || !value.active) return;
            const labels = { resolving: 'Preparing installation', downloading: 'Downloading', verifying: 'Verifying image', extracting: 'Extracting image', importing: 'Importing into WSL', finalizing: 'Confirming WSL registration' };
            status.textContent = labels[value.phase] || 'Preparing installation';
            if (Number.isFinite(value.total) && value.total > 0 && Number.isFinite(value.completed)) {
              if (value.phase === 'extracting') {
                const estimated = Math.min(99, 100 * value.completed / value.total);
                progress.max = 100; progress.value = estimated;
                detail.textContent = `About ${estimated.toFixed(1)}% · ${(value.completed / 1048576).toFixed(1)} MiB extracted (estimate)`;
              } else {
                progress.max = value.total; progress.value = Math.min(value.completed, value.total);
                const perFile = value.phase === 'downloading' ? ' of current file' : '';
                detail.textContent = `${(100 * Math.min(value.completed, value.total) / value.total).toFixed(1)}%${perFile} · ${(value.completed / 1048576).toFixed(1)} / ${(value.total / 1048576).toFixed(1)} MiB${value.item ? ` · ${value.item}` : ''}`;
              }
            } else {
              progress.removeAttribute('value');
              detail.textContent = value.phase === 'extracting' ? `${(value.completed / 1048576).toFixed(1)} MiB extracted` : '';
            }
          } catch (error) {
            if (!finished) {
              status.textContent = 'Progress temporarily unavailable';
              progress.removeAttribute('value');
              detail.textContent = `The installation is still running. ${error}`;
            }
          }
          finally { polling = false; }
        }, 500);
        try {
          const result = JSON.parse(await invoke('install_release', { tag: release.tag }));
          finished = true;
          if (typeof result.distro_name !== 'string' || typeof result.registration_guid !== 'string') throw new Error('Import returned an incomplete identity; refresh before retrying.');
          installedGuid = result.registration_guid;
          await refreshAfterMutation();
          status.textContent = `Installed ${result.distro_name}`;
          progress.max = 1; progress.value = 1;
          detail.textContent = `Complete account setup in a terminal: wsld.exe -d "${result.distro_name}"`;
          note.textContent = '';
          card.classList.add('task-success');
          installTaskDismissTimer = setTimeout(() => { card.hidden = true; installTaskDismissTimer = null; }, 10000);
        } catch (error) {
          finished = true;
          progress.hidden = true;
          const message = String(error);
          if (message.includes('WSL 3.0.3 or later') || message.includes('WSL is unavailable') || message.includes('Cannot start WSL') || message.includes('WSL status check failed') || message.includes('Cannot check WSL status') || message.includes('Windows hypervisor is not running') || message.includes('Cannot verify Windows virtualization') || message.includes('Cannot determine the WSL')) {
            status.textContent = 'Windows or WSL setup needs attention';
            detail.textContent = message;
            note.textContent = 'Complete the steps above, then try again.';
            $('environment-title').textContent = status.textContent;
            $('environment-message').textContent = message;
            $('environment-next-step').textContent = note.textContent;
            if (!$('environment-dialog').open) $('environment-dialog').showModal();
          } else if (message.includes('No supported graphics path')) {
            status.textContent = 'Graphics requirements not met';
            detail.textContent = message;
            note.textContent = 'Update your graphics driver, then try again.';
            $('environment-title').textContent = status.textContent;
            $('environment-message').textContent = message;
            $('environment-next-step').textContent = note.textContent;
            if (!$('environment-dialog').open) $('environment-dialog').showModal();
          } else if (message.includes('Cannot check graphics') || message.includes('Cannot determine graphics') || message.includes('Cannot determine the graphics adapter') || message.includes('Cannot read graphics')) {
            status.textContent = 'Graphics check could not be completed';
            detail.textContent = message;
            note.textContent = 'Resolve the issue above, then try again.';
            $('environment-title').textContent = status.textContent;
            $('environment-message').textContent = message;
            $('environment-next-step').textContent = note.textContent;
            if (!$('environment-dialog').open) $('environment-dialog').showModal();
          } else if (message.includes('Insufficient free space for import')) {
            status.textContent = 'Not enough disk space';
            detail.textContent = message;
            note.textContent = 'Free up space, then choose Install desktop to try again. WSX will verify and reuse the downloaded image if it is still available.';
          } else {
            status.textContent = 'Installation did not complete';
            detail.textContent = message;
            note.textContent = 'Refresh the distribution list before retrying.';
          }
          card.classList.add('task-error');
          const dismiss = element('button', 'task-dismiss', 'Dismiss');
          dismiss.type = 'button';
          dismiss.addEventListener('click', () => { card.hidden = true; });
          card.append(dismiss);
          await refreshInventory();
        }
        finally { clearInterval(timer); installPending = false; $('install-close').disabled = false; $('add-distro').disabled = false; for (const b of $('install-content').querySelectorAll('button')) b.disabled = false; }
      });
      row.append(button); $('install-content').append(row);
    }
    for (const release of catalog.rejected) {
      $('install-content').append(element('p', 'release-unavailable', `${release.tag}: unavailable — ${release.reason}`));
    }
    $('install-status').textContent = catalog.rejected.length ? 'Some stable releases could not be admitted.' : '';
  } catch (error) { if (request === catalogRequest) $('install-status').textContent = `Unable to load stable releases: ${error}`; }
});
$('install-close').addEventListener('click', () => { if (!installPending) { catalogRequest++; $('install-dialog').close(); } });
$('environment-close').addEventListener('click', () => $('environment-dialog').close());
$('environment-dialog').addEventListener('cancel', event => event.preventDefault());
$('install-dialog').addEventListener('cancel', event => { if (installPending) event.preventDefault(); else catalogRequest++; });
