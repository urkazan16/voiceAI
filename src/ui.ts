export type UiLang = "en" | "ru";

const NAV_EN: { id: string; label: string }[] = [
  { id: "home", label: "Pipeline" },
  { id: "settings", label: "Settings" },
  { id: "models", label: "Models" },
  { id: "dictionary", label: "Dictionary" },
  { id: "snippets", label: "Snippets" },
  { id: "profiles", label: "Profiles" },
  { id: "personalization", label: "Personalization" },
  { id: "history", label: "History" },
  { id: "diagnostics", label: "Diagnostics" },
  { id: "privacy", label: "Privacy" },
];

const NAV_RU: { id: string; label: string }[] = [
  { id: "home", label: "Конвейер" },
  { id: "settings", label: "Настройки" },
  { id: "models", label: "Модели" },
  { id: "dictionary", label: "Словарь" },
  { id: "snippets", label: "Фрагменты" },
  { id: "profiles", label: "Профили" },
  { id: "personalization", label: "Персонализация" },
  { id: "history", label: "История" },
  { id: "diagnostics", label: "Диагностика" },
  { id: "privacy", label: "Приватность" },
];

export const NAV = NAV_EN;

export function navItems(lang: string): { id: string; label: string }[] {
  return lang === "ru" ? NAV_RU : NAV_EN;
}

export function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1024 * 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  return `${(size / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
