import { invoke } from "@tauri-apps/api/core";
import { open } from '@tauri-apps/plugin-dialog';
import { warn, debug, trace, info, error } from '@tauri-apps/plugin-log';

function forwardConsole(
  fnName: 'log' | 'debug' | 'info' | 'warn' | 'error',
  logger: (message: string) => Promise<void>
) {
  const original = console[fnName];
  console[fnName] = (message) => {
    original(message);
    logger(message);
  };
}

forwardConsole('log', trace);
forwardConsole('debug', debug);
forwardConsole('info', info);
forwardConsole('warn', warn);
forwardConsole('error', error);

let dirPickedEl: HTMLElement | null;
let dir: string | null;

async function pickDir() {
  if (dirPickedEl) {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    dir = await open({
      multiple: false,
      directory: true,
    });
    console.log("TEST: " + dir);
    dirPickedEl.textContent = dir;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  dirPickedEl = document.querySelector("#dir-picked");
  document.querySelector("#dir-form")?.addEventListener("submit", (e) => {
    e.preventDefault();
    pickDir();
  });
});
