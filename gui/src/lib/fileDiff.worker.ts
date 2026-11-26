import { fileHunks } from "./fileDiff";

self.onmessage = (event: MessageEvent<{ before: string; after: string }>) => {
  self.postMessage(fileHunks(event.data.before, event.data.after));
};
