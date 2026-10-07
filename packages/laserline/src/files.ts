/** Contents of a file the user picked (e.g. from `<input type="file">` or a drop). */
export async function readFileBytes(file: Blob): Promise<Uint8Array> {
  return new Uint8Array(await file.arrayBuffer());
}

/** Offers `data` as a download named `fileName`. */
export function downloadBytes(data: Uint8Array | string, fileName: string, type = 'application/octet-stream'): void {
  const blob = new Blob([typeof data === 'string' ? data : new Uint8Array(data)], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = fileName;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
