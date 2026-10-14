import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DPI, HEAD_WIDTH, mmToPixels, monochrome, packLandscape } from "./bitmap";
import "./style.css";

type Device = { id: string; name: string; address: string; rssi: number | null };
type Printer = { device: Device; modelId: number; protocolVersion: number | null; dpi: number; headWidth: number };
type Progress = { copy: number; copies: number; stage: string; percent: number };
type LogEntry = { level: string; message: string };
type Mode = "text" | "image" | "mixed";
type Settings = {
  text: string; length: number; paperWidth: number; fontSize: number; bold: boolean;
  font: string; align: "left" | "center"; mode: Mode; threshold: number;
  density: number; copies: number; flip: boolean; offset: number; imageData: string;
};

const desktop = isTauri();
const STORE_KEY = "d110-studio.editor.v1";
const defaults: Settings = {
  text: "Зроблено вдома\nNIIMBOT D110", length: 30, paperWidth: 15, fontSize: 24,
  bold: true, font: "sans", align: "center", mode: "text", threshold: 160,
  density: 3, copies: 1, flip: false, offset: 0, imageData: "",
};
const fonts: Record<string, string> = {
  sans: '"DejaVu Sans", sans-serif', mono: '"DejaVu Sans Mono", monospace', serif: '"DejaVu Serif", serif',
};
const settings: Settings = loadSettings();
let connected: Printer | null = null;
let busy = false;
let printing = false;
let image: HTMLImageElement | null = null;
let lastBitmap: number[] = [];
let imageLoading = false;
let imageGeneration = 0;
let eventReady = !desktop;

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <aside class="sidebar">
    <a class="brand" href="#" aria-label="D110 Studio"><span class="brand-icon">D</span><span>D110<span class="brand-light">STUDIO</span></span></a>
    <div class="sidebar-intro">Маленькі етикетки.<br>Порядок у великих справах.</div>
    <div class="section-label">ПРИНТЕР</div>
    <section class="printer-card">
      <div class="printer-illustration" aria-hidden="true"><div class="printer-slot"></div><div class="printer-label">D110</div><div class="printer-button"></div></div>
      <div class="printer-title">NIIMBOT D110</div>
      <div id="connection-badge" class="badge"><span class="dot"></span><span id="connection-text">Не підключено</span></div>
      <p id="device-detail" class="muted">Bluetooth · 203 dpi</p>
      <button id="scan" class="button secondary wide">Знайти принтер <span>↗</span></button>
      <div id="device-picker" hidden>
        <label for="devices">Доступні принтери</label>
        <select id="devices"></select>
        <button id="connect" class="button primary wide">Підключити</button>
      </div>
      <button id="disconnect" class="button quiet wide" hidden>Відключити</button>
      <p id="scan-help" class="small muted">Увімкни принтер і Bluetooth. Закрий застосунок NIIMBOT на телефоні.</p>
    </section>
    <div class="section-label template-heading">ШВИДКИЙ ПОЧАТОК</div>
    <button class="template" data-template="cable"><span class="template-mark">↳</span><span>Кабелі<span class="muted">USB-C, живлення, мережа</span></span></button>
    <button class="template" data-template="storage"><span class="template-mark">▤</span><span>Зберігання<span class="muted">Коробки й дрібниці</span></span></button>
    <button class="template" data-template="test"><span class="template-mark">⊞</span><span>Тестова етикетка<span class="muted">Перевірка першого друку</span></span></button>
    <div class="sidebar-footer"><span class="dot"></span> Локально на твоєму ноутбуці<span class="version">v0.1 · Linux</span></div>
  </aside>
  <main>
    <header class="topbar"><span class="breadcrumb">Робочий стіл <span>/</span> Нова етикетка</span><button id="export" class="button quiet">Зберегти PNG ↓</button></header>
    <div class="workspace">
      <div class="page-heading"><div><div class="eyebrow">МІСЦЕ ДЛЯ КОЖНОЇ РЕЧІ</div><h1>Створи свою етикетку<span>.</span></h1><p class="muted">Напиши, приміряй, надрукуй.</p></div><span id="dimension-tag" class="dimension-tag">30 × 15 мм</span></div>
      <section class="preview-section" aria-label="Попередній перегляд етикетки">
        <div class="preview-top"><span class="section-label">ПОПЕРЕДНІЙ ПЕРЕГЛЯД</span><span class="preview-live"><span class="dot"></span> Як на папері</span></div>
        <div class="preview-stage"><div class="preview-measure" id="length-label">30 мм</div><div id="paper" class="paper"><canvas id="preview" width="240" height="96" aria-label="Зображення для друку"></canvas></div><div id="preview-warning" class="preview-warning"></div></div>
        <div class="preview-bottom"><span id="raster-detail">203 dpi · 96 × 240 px</span><span>Друк чорно-білий</span></div>
      </section>
      <div class="editor-grid">
        <section class="panel content-panel"><div class="panel-heading"><h2>Вміст</h2><select id="mode" aria-label="Тип вмісту"><option value="text">Текст</option><option value="image">Зображення</option><option value="mixed">Текст + зображення</option></select></div>
          <div id="text-editor"><label for="text">Текст етикетки</label><textarea id="text" maxlength="600" rows="3" spellcheck="false" placeholder="Що тут зберігається?"></textarea>
          <div class="field-row"><div class="field grow"><label for="font">Шрифт</label><select id="font"><option value="sans">Sans</option><option value="mono">Mono</option><option value="serif">Serif</option></select></div><div class="field narrow"><label for="font-size">Розмір, px</label><input id="font-size" type="number" min="8" max="48" step="1"></div><label class="toggle-label bold-toggle"><input id="bold" type="checkbox"><span>Ж</span></label></div>
          <div class="field-row"><label class="inline-label" for="align">Вирівнювання</label><select id="align"><option value="center">По центру</option><option value="left">Ліворуч</option></select></div></div>
          <div id="image-editor" hidden><label for="image-input" class="upload-zone">↥ <strong>Додати зображення</strong><span>PNG, JPEG або WebP · до 10 МБ</span><input id="image-input" type="file" accept="image/png,image/jpeg,image/webp"></label><div class="image-actions"><span id="image-name" class="small muted">Зображення не вибрано</span><button id="remove-image" class="text-button">Прибрати</button></div><label for="threshold">Поріг чорно-білого <output id="threshold-value"></output></label><input id="threshold" type="range" min="40" max="220" step="1"></div>
        </section>
        <section class="panel"><div class="panel-heading"><h2>Папір і друк</h2><span class="small muted">D110</span></div>
          <div class="field-row"><div class="field grow"><label for="length">Довжина, мм</label><select id="length"><option value="20">20</option><option value="25">25</option><option value="30">30</option><option value="40">40</option><option value="50">50</option><option value="60">60</option><option value="70">70</option><option value="80">80</option><option value="100">100</option></select></div><div class="field grow"><label for="paper-width">Ширина, мм</label><select id="paper-width"><option value="12">12</option><option value="14">14</option><option value="15">15</option></select></div></div>
          <p class="small muted paper-note">Вибери розмір установленої етикетки. Доступна ширина друку — близько 12 мм.</p>
          <div class="field-row"><div class="field grow"><label for="density">Щільність</label><select id="density"><option value="1">1 · Легка</option><option value="2">2</option><option value="3">3 · Звичайна</option><option value="4">4</option><option value="5">5 · Темна</option></select></div><div class="field narrow"><label for="copies">Копії</label><input id="copies" type="number" min="1" max="20" step="1"></div></div>
          <details class="calibration"><summary>Орієнтація та зміщення</summary><label class="check-row"><input id="flip" type="checkbox">Повернути на 180°</label><div class="field-row"><label class="inline-label" for="offset">Зміщення, px</label><input id="offset" type="number" min="-32" max="32" step="1"></div><p class="small muted">1 px ≈ 0,125 мм уздовж етикетки.</p></details>
        </section>
      </div>
      <div class="print-bar"><div><div id="print-title" class="print-title">Етикетка готова до друку</div><div id="print-detail" class="small muted">Підключи принтер, щоб почати.</div></div><div class="print-actions"><button id="cancel" class="button secondary" hidden>Зупинити</button><button id="print" class="button primary print-button" disabled>Надрукувати <span>↗</span></button></div></div>
      <progress id="progress" max="100" value="0" hidden aria-label="Прогрес друку"></progress>
      <div id="notice" class="notice" role="status" aria-live="polite" hidden></div>
      <details class="diagnostics"><summary>Журнал принтера <span id="log-count">0</span></summary><div class="log-toolbar"><span class="small muted">Команди, відповіді та помилки Bluetooth</span><button id="save-log" class="text-button">Зберегти журнал ↓</button></div><ol id="logs"></ol></details>
    </div>
  </main>`;

function el<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing element ${id}`);
  return element as T;
}

function loadSettings(): Settings {
  try {
    const value = JSON.parse(localStorage.getItem(STORE_KEY) ?? "null") as Partial<Settings> | null;
    if (!value) return { ...defaults };
    const next = { ...defaults, ...value };
    const validNumber = (v: number, min: number, max: number) => Number.isInteger(v) && v >= min && v <= max;
    if (!validNumber(next.length, 20, 100) || ![20, 25, 30, 40, 50, 60, 70, 80, 100].includes(next.length)) next.length = defaults.length;
    if (![12, 14, 15].includes(next.paperWidth)) next.paperWidth = defaults.paperWidth;
    if (!validNumber(next.fontSize, 8, 48)) next.fontSize = defaults.fontSize;
    if (!validNumber(next.threshold, 40, 220)) next.threshold = defaults.threshold;
    if (!validNumber(next.offset, -32, 32)) next.offset = 0;
    if (!validNumber(next.copies, 1, 20)) next.copies = 1;
    if (!validNumber(next.density, 1, 5)) next.density = 3;
    if (!["text", "image", "mixed"].includes(next.mode)) next.mode = "text";
    if (!["left", "center"].includes(next.align)) next.align = "center";
    if (!Object.hasOwn(fonts, next.font)) next.font = "sans";
    next.text = typeof next.text === "string" ? next.text.slice(0, 600) : defaults.text;
    next.imageData = typeof next.imageData === "string" && /^data:image\/png;base64,/.test(next.imageData) && next.imageData.length < 2_000_000 ? next.imageData : "";
    next.bold = next.bold === true;
    next.flip = next.flip === true;
    return next;
  } catch { return { ...defaults }; }
}

function persist(): void {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(settings)); }
  catch { notify("Не вдалося зберегти етикетку локально. Можна зберегти її як PNG.", "error"); }
}

function fillControls(): void {
  for (const [id, key] of Object.entries({
    text: "text", length: "length", "paper-width": "paperWidth", "font-size": "fontSize",
    font: "font", align: "align", mode: "mode", threshold: "threshold", density: "density", copies: "copies", offset: "offset",
  })) (el<HTMLInputElement | HTMLSelectElement>(id)).value = String(settings[key as keyof Settings]);
  el<HTMLInputElement>("bold").checked = settings.bold;
  el<HTMLInputElement>("flip").checked = settings.flip;
}

function notify(message: string, kind: "info" | "error" | "success" = "info"): void {
  const notice = el("notice");
  notice.textContent = message;
  notice.className = `notice ${kind}`;
  notice.hidden = false;
}

const logEntries: string[] = [];
function addLog(entry: LogEntry): void {
  const time = new Date().toLocaleTimeString("uk-UA");
  const text = `${time} [${entry.level}] ${entry.message}`;
  logEntries.push(text);
  if (logEntries.length > 500) logEntries.shift();
  const item = document.createElement("li");
  item.className = entry.level;
  item.textContent = text;
  el("logs").append(item);
  while (el("logs").children.length > 150) el("logs").firstElementChild?.remove();
  el("log-count").textContent = String(logEntries.length);
  el("logs").scrollTop = el("logs").scrollHeight;
}

function errorMessage(error: unknown): string { return error instanceof Error ? error.message : String(error); }

async function operation(action: () => Promise<void>): Promise<void> {
  if (busy) return;
  busy = true;
  updateState();
  try { await action(); }
  catch (error) {
    const message = errorMessage(error);
    addLog({ level: "error", message });
    notify(message, "error");
  } finally { busy = false; updateState(); }
}

function updateState(): void {
  el("connection-text").textContent = connected ? "Підключено" : "Не підключено";
  el("connection-badge").classList.toggle("online", !!connected);
  el("device-detail").textContent = connected ? connected.device.name : "Bluetooth · 203 dpi";
  el<HTMLButtonElement>("scan").disabled = busy || !desktop || !eventReady;
  el<HTMLButtonElement>("connect").disabled = busy || !desktop;
  el<HTMLButtonElement>("disconnect").disabled = busy;
  el("disconnect").hidden = !connected;
  el<HTMLButtonElement>("print").disabled = busy || !connected || !lastBitmap.some(v => v !== 0) || imageLoading;
  el("cancel").hidden = !printing;
  el<HTMLButtonElement>("cancel").disabled = !printing;
  el<HTMLProgressElement>("progress").hidden = !printing;
  // Keep the visible preview stable while the exact bitmap is being printed.
  for (const control of document.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(".editor-grid input, .editor-grid select, .editor-grid textarea")) control.disabled = printing;
  for (const button of document.querySelectorAll<HTMLButtonElement>(".template, #remove-image")) button.disabled = printing;
  if (!printing) {
    el("print-title").textContent = lastBitmap.some(v => v !== 0) ? "Етикетка готова до друку" : "Додай вміст етикетки";
    el("print-detail").textContent = connected ? `${settings.copies} коп. · ${settings.length} × ${settings.paperWidth} мм · щільність ${settings.density}` : desktop ? "Підключи принтер, щоб почати." : "Перегляд у браузері. Bluetooth доступний у застосунку Tauri.";
  }
}

function render(): void {
  const canvas = el<HTMLCanvasElement>("preview");
  canvas.width = mmToPixels(settings.length);
  canvas.height = HEAD_WIDTH;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("Canvas недоступний.");
  ctx.fillStyle = "white"; ctx.fillRect(0, 0, canvas.width, canvas.height);
  const margin = 6;
  const room = canvas.width - margin * 2;
  let textX = margin, textWidth = room;
  if (settings.mode !== "text" && image) {
    const maxWidth = settings.mode === "mixed" ? room * 0.34 : room;
    const scale = Math.min(maxWidth / image.width, (HEAD_WIDTH - margin * 2) / image.height);
    const w = image.width * scale, h = image.height * scale;
    const x = settings.mode === "mixed" ? margin : (canvas.width - w) / 2;
    ctx.drawImage(image, x, (HEAD_WIDTH - h) / 2, w, h);
    if (settings.mode === "mixed") { textX = margin + maxWidth + 8; textWidth = room - maxWidth - 8; }
  }
  let warning = "";
  if (settings.mode !== "image" && settings.text.trim()) {
    const lines = settings.text.split("\n").slice(0, 12);
    const family = fonts[settings.font] ?? fonts.sans;
    let size = settings.fontSize;
    const setFont = () => { ctx.font = `${settings.bold ? 700 : 400} ${size}px ${family}`; };
    setFont();
    while (size > 8 && (lines.length * size * 1.2 > HEAD_WIDTH - margin * 2 || lines.some(line => ctx.measureText(line).width > textWidth))) {
      size -= 1; setFont();
    }
    ctx.fillStyle = "black"; ctx.textBaseline = "middle"; ctx.textAlign = settings.align;
    const lineHeight = size * 1.2;
    const startY = (HEAD_WIDTH - (lines.length - 1) * lineHeight) / 2;
    ctx.save(); ctx.beginPath(); ctx.rect(textX, margin, textWidth, HEAD_WIDTH - margin * 2); ctx.clip();
    lines.forEach((line, i) => ctx.fillText(line, settings.align === "center" ? textX + textWidth / 2 : textX, startY + i * lineHeight));
    ctx.restore();
    if (size < settings.fontSize) warning = `Текст зменшено до ${size} px, щоб умістити на етикетці.`;
    if (settings.text.split("\n").length > 12 || lines.length * lineHeight > HEAD_WIDTH - margin * 2 || lines.some(line => ctx.measureText(line).width > textWidth)) warning = "Текст не вміщується повністю. Скороти його або збільш довжину етикетки.";
  }
  if (settings.mode !== "text" && !image) warning = imageLoading ? "Завантаження зображення…" : "Додай зображення для цього макета.";
  const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const binary = monochrome(pixels.data, canvas.width, canvas.height, settings.threshold, settings.flip, settings.offset);
  ctx.putImageData(new ImageData(binary, canvas.width, canvas.height), 0, 0);
  lastBitmap = packLandscape(binary, canvas.width, canvas.height);
  const paper = el("paper");
  paper.style.aspectRatio = `${settings.length} / ${settings.paperWidth}`;
  paper.style.width = `min(100%, ${220 * settings.length / settings.paperWidth}px)`;
  canvas.style.height = `${Math.min(100, (HEAD_WIDTH / mmToPixels(settings.paperWidth)) * 100)}%`;
  el("dimension-tag").textContent = `${settings.length} × ${settings.paperWidth} мм`;
  el("length-label").textContent = `${settings.length} мм`;
  el("raster-detail").textContent = `${DPI} dpi · ${HEAD_WIDTH} × ${canvas.width} px`;
  el("preview-warning").textContent = warning;
  el("text-editor").hidden = settings.mode === "image";
  el("image-editor").hidden = settings.mode === "text";
  el("threshold-value").textContent = String(settings.threshold);
  el("image-name").textContent = image ? `${image.width} × ${image.height} px` : "Зображення не вибрано";
  updateState();
}

function numberValue(id: string, fallback: number, min: number, max: number): number {
  const field = el<HTMLInputElement>(id);
  const value = Number(field.value);
  if (!Number.isInteger(value) || value < min || value > max || !field.value.trim()) return fallback;
  return value;
}

function readControls(): void {
  settings.text = el<HTMLTextAreaElement>("text").value;
  settings.length = numberValue("length", settings.length, 20, 100);
  settings.paperWidth = numberValue("paper-width", settings.paperWidth, 12, 15);
  settings.fontSize = numberValue("font-size", settings.fontSize, 8, 48);
  settings.threshold = numberValue("threshold", settings.threshold, 40, 220);
  settings.density = numberValue("density", settings.density, 1, 5);
  settings.copies = numberValue("copies", settings.copies, 1, 20);
  settings.offset = numberValue("offset", settings.offset, -32, 32);
  settings.bold = el<HTMLInputElement>("bold").checked;
  settings.flip = el<HTMLInputElement>("flip").checked;
  settings.font = el<HTMLSelectElement>("font").value;
  settings.mode = el<HTMLSelectElement>("mode").value as Mode;
  settings.align = el<HTMLSelectElement>("align").value as Settings["align"];
  render(); persist();
}

async function loadImage(data: string): Promise<void> {
  const generation = ++imageGeneration;
  imageLoading = true; updateState();
  try {
    const loaded = new Image();
    loaded.src = data;
    await loaded.decode();
    if (generation !== imageGeneration) return;
    image = loaded;
  } finally {
    if (generation === imageGeneration) { imageLoading = false; render(); }
  }
}

function download(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a"); a.href = url; a.download = name;
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 3000);
}

async function saveArtifact(blob: Blob, name: string, kind: "png" | "log"): Promise<void> {
  if (!desktop) { download(blob, name); return; }
  const path = await invoke<string | null>("save_artifact", { kind, data: Array.from(new Uint8Array(await blob.arrayBuffer())) });
  if (path) notify(`Файл збережено: ${path}`, "success");
}

el("scan").addEventListener("click", () => void operation(async () => {
  el("scan").textContent = "Шукаю…";
  el("device-picker").hidden = true;
  try {
    const devices = await invoke<Device[]>("scan_printers");
    const select = el<HTMLSelectElement>("devices"); select.replaceChildren();
    for (const device of devices) {
      const option = document.createElement("option"); option.value = device.id;
      option.textContent = `${device.name} · ${device.address}`; select.append(option);
    }
    el("device-picker").hidden = devices.length === 0;
    notify(devices.length ? "Вибери свій принтер і натисни «Підключити»." : "D110 не знайдено. Увімкни принтер, закрий NIIMBOT на телефоні й повтори пошук.");
  } finally { el("scan").textContent = "Знайти принтер ↗"; }
}));

el("connect").addEventListener("click", () => void operation(async () => {
  const id = el<HTMLSelectElement>("devices").value;
  if (!id) return;
  connected = null;
  notify("Підключення та перевірка моделі принтера…");
  connected = await invoke<Printer>("connect_printer", { id });
  el("device-picker").hidden = true;
  notify(`Підключено ${connected.device.name}. Можна друкувати.`, "success");
}));

el("disconnect").addEventListener("click", () => void operation(async () => {
  try { await invoke("disconnect_printer"); }
  finally { connected = null; }
}));

el("print").addEventListener("click", () => void operation(async () => {
  if (!connected || !lastBitmap.some(v => v !== 0)) return;
  // Check inputs again: number fields may be temporarily empty during editing.
  for (const id of ["font-size", "copies", "offset"]) {
    if (!el<HTMLInputElement>(id).checkValidity()) throw new Error("Перевір числові значення в налаштуваннях етикетки.");
  }
  printing = true; updateState();
  el<HTMLProgressElement>("progress").value = 0;
  el("notice").hidden = true;
  try {
    const report = await invoke<{ completedCopies: number }>("print_label", {
      request: { width: HEAD_WIDTH, height: mmToPixels(settings.length), bitmap: lastBitmap.slice(), density: settings.density, copies: settings.copies },
    });
    notify(`Надруковано копій: ${report.completedCopies}.`, "success");
  } catch (error) { connected = null; throw error; }
  finally { printing = false; }
}));

el("cancel").addEventListener("click", () => {
  el<HTMLButtonElement>("cancel").disabled = true;
  void invoke("cancel_print").then(() => notify("Зупиняю друк і завершую поточне завдання…"))
    .catch(error => { notify(errorMessage(error), "error"); el<HTMLButtonElement>("cancel").disabled = false; });
});

for (const control of document.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(".editor-grid input:not([type=file]), .editor-grid select, .editor-grid textarea")) {
  control.addEventListener("input", readControls);
  control.addEventListener("change", () => { readControls(); fillControls(); });
}

el("image-input").addEventListener("change", () => {
  const file = el<HTMLInputElement>("image-input").files?.[0];
  if (!file) return;
  void (async () => {
    if (!["image/png", "image/jpeg", "image/webp"].includes(file.type) || file.size > 10 * 1024 * 1024) throw new Error("Вибери PNG, JPEG або WebP розміром до 10 МБ.");
    imageLoading = true; updateState();
    const bitmap = await createImageBitmap(file);
    try {
      const canvas = document.createElement("canvas");
      const scale = Math.min(1, 1024 / Math.max(bitmap.width, bitmap.height));
      canvas.width = Math.max(1, Math.round(bitmap.width * scale)); canvas.height = Math.max(1, Math.round(bitmap.height * scale));
      canvas.getContext("2d")!.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
      const data = canvas.toDataURL("image/png");
      if (data.length > 2_000_000) throw new Error("Зображення надто складне для збереження. Спробуй простіший файл.");
      await loadImage(data);
      settings.imageData = data;
      if (settings.mode === "text") settings.mode = "mixed";
      fillControls(); render(); persist();
    } finally { bitmap.close(); }
  })().catch(error => notify(errorMessage(error), "error")).finally(() => {
    imageLoading = false; updateState(); el<HTMLInputElement>("image-input").value = "";
  });
});

el("remove-image").addEventListener("click", () => {
  imageGeneration++; image = null; imageLoading = false; settings.imageData = "";
  render(); persist();
});

for (const button of document.querySelectorAll<HTMLButtonElement>(".template")) button.addEventListener("click", () => {
  settings.mode = "text"; settings.bold = true; settings.align = "center"; settings.flip = false; settings.offset = 0;
  if (button.dataset.template === "cable") { settings.text = "USB-C\n20 W"; settings.length = 30; settings.fontSize = 26; }
  if (button.dataset.template === "storage") { settings.text = "ЕЛЕКТРОНІКА\nРезистори · 10 кОм"; settings.length = 40; settings.fontSize = 24; }
  if (button.dataset.template === "test") { settings.text = "D110\nТЕСТ 123"; settings.fontSize = 24; }
  fillControls(); render(); persist();
});

el("export").addEventListener("click", () => {
  // Export the landscape artwork; the Tauri backend receives the rotated bitmap.
  el<HTMLCanvasElement>("preview").toBlob(blob => {
    if (blob) void saveArtifact(blob, `label-${settings.length}x${settings.paperWidth}.png`, "png").catch(error => notify(errorMessage(error), "error"));
  }, "image/png");
});
el("save-log").addEventListener("click", () => {
  void saveArtifact(new Blob([`D110 Studio 0.1.0 · Linux\n${new Date().toISOString()}\n\n${logEntries.join("\n")}\n`], { type: "text/plain;charset=utf-8" }), "d110-printer.log", "log").catch(error => notify(errorMessage(error), "error"));
});

async function initEvents(): Promise<void> {
  if (!desktop) { notify("Режим перегляду: редактор і PNG працюють у браузері. Для Bluetooth запусти застосунок Tauri."); return; }
  try {
    await listen<LogEntry>("printer-log", event => addLog(event.payload));
    await listen<Progress>("print-progress", event => {
      const p = event.payload;
      const titles: Record<string, string> = { configuring: "Налаштовую принтер", uploading: "Передаю етикетку", printing: "Друкую", complete: "Копія готова" };
      el("print-title").textContent = titles[p.stage] ?? "Друк…";
      el("print-detail").textContent = `Копія ${p.copy} з ${p.copies} · ${p.percent}%`;
      const fraction = p.stage === "configuring" ? 0 : p.stage === "uploading" ? p.percent * 0.45 : p.stage === "printing" ? 45 + p.percent * 0.5 : 100;
      el<HTMLProgressElement>("progress").value = ((p.copy - 1) * 100 + fraction) / p.copies;
    });
    eventReady = true; updateState();
  } catch (error) { notify(`Не вдалося підключити події застосунку: ${errorMessage(error)}`, "error"); }
}

setInterval(() => {
  if (!desktop || busy || !connected) return;
  void invoke<Printer | null>("connection_status").then(printer => {
    if (busy) return;
    if (!printer && connected) notify("З'єднання з принтером втрачено. Повтори пошук і підключення.", "error");
    connected = printer; updateState();
  }).catch(error => {
    if (busy) return;
    connected = null; updateState(); notify(errorMessage(error), "error");
  });
}, 5000);

fillControls(); render();
if (settings.imageData) void loadImage(settings.imageData).catch(() => {
  settings.imageData = ""; image = null; render(); persist();
});
void initEvents();
