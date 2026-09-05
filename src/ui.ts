export const NAV: { id: string; label: string }[] = [
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

export function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1024 * 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  return `${(size / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
