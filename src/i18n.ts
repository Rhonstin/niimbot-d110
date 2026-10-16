export type Language = "en" | "uk";
export const LANGUAGE_KEY = "d110-studio.language.v1";

export const ukrainian = {
  "Language": "Мова",
  "Small labels.": "Маленькі етикетки.",
  "A place for everything.": "Порядок у великих справах.",
  "PRINTER": "ПРИНТЕР",
  "Not connected": "Не підключено",
  "Connected": "Підключено",
  "Find printer": "Знайти принтер",
  "Available printers": "Доступні принтери",
  "Connect": "Підключити",
  "Disconnect": "Відключити",
  "Turn on the printer and Bluetooth. Close NIIMBOT on your phone.": "Увімкни принтер і Bluetooth. Закрий NIIMBOT на телефоні.",
  "QUICK START": "ШВИДКИЙ ПОЧАТОК",
  "Cables": "Кабелі",
  "USB-C, power, network": "USB-C, живлення, мережа",
  "Storage": "Зберігання",
  "Boxes and small things": "Коробки й дрібниці",
  "Test label": "Тестова етикетка",
  "Check your first print": "Перевірка першого друку",
  "Local on your device": "Локально на твоєму пристрої",
  "Workspace": "Робочий стіл",
  "New label": "Нова етикетка",
  "Save PNG ↓": "Зберегти PNG ↓",
  "A PLACE FOR EVERYTHING": "МІСЦЕ ДЛЯ КОЖНОЇ РЕЧІ",
  "Make your own label": "Створи свою етикетку",
  "Write, preview, print.": "Напиши, приміряй, надрукуй.",
  "Label preview": "Попередній перегляд етикетки",
  "PREVIEW": "ПОПЕРЕДНІЙ ПЕРЕГЛЯД",
  "As on paper": "Як на папері",
  "Print artwork": "Зображення для друку",
  "Monochrome printing": "Друк чорно-білий",
  "Content": "Вміст",
  "Content type": "Тип вмісту",
  "Text": "Текст",
  "Image": "Зображення",
  "Text + image": "Текст + зображення",
  "Label text": "Текст етикетки",
  "What belongs here?": "Що тут зберігається?",
  "Font": "Шрифт",
  "Size, px": "Розмір, px",
  "Bold": "Жирний",
  "Alignment": "Вирівнювання",
  "Center": "По центру",
  "Left": "Ліворуч",
  "Add image": "Додати зображення",
  "PNG, JPEG or WebP · up to 10 MB": "PNG, JPEG або WebP · до 10 МБ",
  "No image selected": "Зображення не вибрано",
  "Remove": "Прибрати",
  "Monochrome threshold": "Поріг чорно-білого",
  "Paper and print": "Папір і друк",
  "Length, mm": "Довжина, мм",
  "Width, mm": "Ширина, мм",
  "Match the loaded label size. Printable width is about 12 mm.": "Вибери розмір установленої етикетки. Доступна ширина друку — близько 12 мм.",
  "Density": "Щільність",
  "1 · Light": "1 · Легка",
  "3 · Normal": "3 · Звичайна",
  "5 · Dark": "5 · Темна",
  "Copies": "Копії",
  "Orientation and offset": "Орієнтація та зміщення",
  "Rotate 180°": "Повернути на 180°",
  "Offset, px": "Зміщення, px",
  "1 px ≈ 0.125 mm along the label.": "1 px ≈ 0,125 мм уздовж етикетки.",
  "Label ready to print": "Етикетка готова до друку",
  "Add label content": "Додай вміст етикетки",
  "Connect your printer to get started.": "Підключи принтер, щоб почати.",
  "Stop": "Зупинити",
  "Print": "Надрукувати",
  "Print progress": "Прогрес друку",
  "Printer log": "Журнал принтера",
  "Bluetooth commands, responses and errors": "Команди, відповіді та помилки Bluetooth",
  "Save log ↓": "Зберегти журнал ↓",
  "Could not save the label locally. You can export it as PNG.": "Не вдалося зберегти етикетку локально. Можна зберегти її як PNG.",
  "Browser preview. Bluetooth is available in the Tauri app.": "Перегляд у браузері. Bluetooth доступний у застосунку Tauri.",
  "{copies} copies · {length} × {width} mm · density {density}": "{copies} коп. · {length} × {width} мм · щільність {density}",
  "Text resized to {size} px to fit the label.": "Текст зменшено до {size} px, щоб умістити на етикетці.",
  "Text does not fit. Shorten it or increase the label length.": "Текст не вміщується повністю. Скороти його або збільш довжину етикетки.",
  "Loading image…": "Завантаження зображення…",
  "Add an image for this layout.": "Додай зображення для цього макета.",
  "{value} mm": "{value} мм",
  "{length} × {width} mm": "{length} × {width} мм",
  "File saved: {path}": "Файл збережено: {path}",
  "Searching…": "Шукаю…",
  "Choose your printer and click Connect.": "Вибери свій принтер і натисни «Підключити».",
  "No D110 found. Turn it on, close NIIMBOT on your phone and search again.": "D110 не знайдено. Увімкни принтер, закрий NIIMBOT на телефоні й повтори пошук.",
  "Connecting and checking printer model…": "Підключення та перевірка моделі принтера…",
  "Connected to {name}. Ready to print.": "Підключено {name}. Можна друкувати.",
  "Check the numeric values in label settings.": "Перевір числові значення в налаштуваннях етикетки.",
  "Printed {copies} copies.": "Надруковано копій: {copies}.",
  "Stopping print and closing the current job…": "Зупиняю друк і завершую поточне завдання…",
  "Choose a PNG, JPEG or WebP file up to 10 MB.": "Вибери PNG, JPEG або WebP розміром до 10 МБ.",
  "Image is too complex to save. Try a simpler file.": "Зображення надто складне для збереження. Спробуй простіший файл.",
  "Preview mode: the editor and PNG export work in your browser. Run the Tauri app for Bluetooth.": "Режим перегляду: редактор і PNG працюють у браузері. Для Bluetooth запусти застосунок Tauri.",
  "Configuring printer": "Налаштовую принтер",
  "Sending label": "Передаю етикетку",
  "Printing": "Друкую",
  "Copy complete": "Копія готова",
  "Copy {copy} of {copies} · {percent}%": "Копія {copy} з {copies} · {percent}%",
  "Could not subscribe to app events: {error}": "Не вдалося підключити події застосунку: {error}",
  "Printer disconnected. Search and connect again.": "З'єднання з принтером втрачено. Повтори пошук і підключення.",
} as const;

export type Message = keyof typeof ukrainian;
let language: Language = "en";

export function resolveLanguage(value: string | null): Language {
  return value === "uk" ? "uk" : "en";
}

export function setLanguage(value: Language): void { language = value; }
export function getLanguage(): Language { return language; }
export function t(message: Message, values: Record<string, string | number> = {}): string {
  const template: string = language === "uk" ? ukrainian[message] : message;
  return template.replace(/\{(\w+)\}/g, (match, key: string) => String(values[key] ?? match));
}

/** Bind static UI copy once. Editable label text is never translated. */
export function bindTranslations(root: HTMLElement): () => void {
  const bindings: Array<() => void> = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    if (node.parentElement?.closest("textarea, script, style")) continue;
    const original = node.textContent ?? "";
    const message = original.trim() as Message;
    if (!Object.hasOwn(ukrainian, message)) continue;
    const prefix = original.slice(0, original.indexOf(message));
    const suffix = original.slice(original.indexOf(message) + message.length);
    bindings.push(() => { if (node.isConnected) node.textContent = prefix + t(message) + suffix; });
  }
  for (const element of root.querySelectorAll<HTMLElement>("[aria-label], [placeholder], [title]")) {
    for (const attr of ["aria-label", "placeholder", "title"]) {
      const message = element.getAttribute(attr) as Message | null;
      if (message && Object.hasOwn(ukrainian, message)) bindings.push(() => element.setAttribute(attr, t(message)));
    }
  }
  return () => {
    document.documentElement.lang = language;
    bindings.forEach(update => update());
  };
}
