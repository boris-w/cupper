// The preload runs in the main frame only (`nodeIntegrationInSubFrames` is
// false): the main document sees this marker, the review document must not.
import { contextBridge } from "electron";

contextBridge.exposeInMainWorld("t104Preload", true);
