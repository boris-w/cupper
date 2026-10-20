// Q26 probe, the Electron side: a window with a button, a text field and a
// same-origin iframe with its own field. It logs every before-input-event
// and input-event of the window, and the page's keydowns with isTrusted; the
// runner reads the logs through `globalThis.q26`.
const { app, BrowserWindow } = require("electron");

const PAGE = `<!doctype html><meta charset="utf-8">
<button id="b">button</button> <input id="main">
<iframe id="f" srcdoc="<input id='inner'>"></iframe>
<script>
  window.keys = [];
  addEventListener("keydown", (e) => keys.push({ key: e.key, isTrusted: e.isTrusted }), true);
</script>`;

const log = { beforeInput: [], inputEvent: [] };

app.whenReady().then(() => {
  const window = new BrowserWindow({ width: 600, height: 400, show: true });
  const contents = window.webContents;
  contents.on("before-input-event", (_event, input) => {
    log.beforeInput.push({ type: input.type, key: input.key, code: input.code });
  });
  contents.on("input-event", (_event, input) => {
    if (input.type !== "mouseMove" && input.type !== "mouseEnter" && input.type !== "mouseLeave") {
      log.inputEvent.push({ ...input });
    }
  });
  globalThis.q26 = {
    log,
    clear: () => {
      log.beforeInput.length = 0;
      log.inputEvent.length = 0;
    },
    send: (event) => contents.sendInputEvent(event),
    focusedFrame: () => {
      const focused = contents.focusedFrame;
      return {
        sameObject: focused === contents.mainFrame,
        sameNode: focused?.frameTreeNodeId === contents.mainFrame.frameTreeNodeId,
        url: focused?.url ?? null,
      };
    },
  };
  void contents.loadURL(`data:text/html,${encodeURIComponent(PAGE)}`);
});
