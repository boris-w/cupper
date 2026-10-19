/** Whether `openExternal` takes the URL: an absolute http or https URL (design 5.7.8). */
export function isExternalUrl(url: string): boolean {
  if (!URL.canParse(url)) {
    return false;
  }
  const { protocol } = new URL(url);
  return protocol === "http:" || protocol === "https:";
}
