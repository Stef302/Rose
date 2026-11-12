"use strict";

const invoke = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
const name = document.getElementById("product-name");
const version = document.getElementById("product-version");
const destination = document.getElementById("destination");
const installButton = document.getElementById("install");
const status = document.getElementById("status");

function showStatus(message, state) {
  status.textContent = message;
  status.dataset.state = state || "";
}

async function initialize() {
  if (!invoke) {
    showStatus("Cette interface doit être lancée dans l'installeur Tauri.", "error");
    return;
  }

  try {
    const info = await invoke("installer_info");
    name.textContent = info.productName;
    version.textContent = `Version ${info.version}`;
    destination.value = info.defaultInstallDir;
    installButton.disabled = false;
    showStatus("Prêt à installer.", "");
  } catch (error) {
    showStatus(`Impossible de charger la configuration : ${error}`, "error");
  }
}

installButton.addEventListener("click", async () => {
  installButton.disabled = true;
  destination.disabled = true;
  showStatus("Installation en cours…", "");

  try {
    const result = await invoke("install_app", { destination: destination.value });
    showStatus(result, "success");
  } catch (error) {
    showStatus(`Échec de l'installation : ${error}`, "error");
  } finally {
    destination.disabled = false;
    installButton.disabled = false;
  }
});

initialize();
