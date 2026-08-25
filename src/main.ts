import { createApp } from "vue";
import App from "./App.vue";

createApp(App).mount("#app");

const startupScreen = document.querySelector<HTMLElement>("#startup-screen");
const finishStartup = () => {
  document.body.classList.add("startup-ready");
  if (!startupScreen) return;
  const removeStartupScreen = () => startupScreen.remove();
  startupScreen.addEventListener("transitionend", removeStartupScreen, { once: true });
  window.setTimeout(removeStartupScreen, 700);
};

window.requestAnimationFrame(() => window.requestAnimationFrame(finishStartup));
