// The main document's preload (design 5.7.1, 5.7.3). It runs sandboxed, so it
// is bundled into one CommonJS file and may only require `electron`. T076
// exposes the Electron ShellAPI here through `contextBridge`.
export {};
