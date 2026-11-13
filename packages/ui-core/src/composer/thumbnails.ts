// The attached images' thumbnails (design 5.7.4): an object URL of the local
// file, by the image's blob hash, while the image is attached. Nothing from
// elsewhere reaches an `<img>` here; each URL is released once its image is
// no longer attached or the composer is gone.

const urls = new Map<string, string>();

export const thumbnail = (hash: string) => urls.get(hash);

export function keepThumbnail(hash: string, file: Blob) {
  if (!urls.has(hash)) urls.set(hash, URL.createObjectURL(file));
}

/** Releases every thumbnail whose hash is not in `attached`. */
export function releaseThumbnails(attached: ReadonlySet<string> = new Set()) {
  for (const [hash, url] of urls) {
    if (attached.has(hash)) continue;
    URL.revokeObjectURL(url);
    urls.delete(hash);
  }
}
