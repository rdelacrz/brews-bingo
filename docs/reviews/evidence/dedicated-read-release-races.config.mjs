import { fileURLToPath } from "node:url";
import base from "../../../backend/vitest.config.js";

// Deliberately separate from the passing suite: five cases reproduce an open blocker.
export default {
  ...base,
  test: {
    ...base.test,
    include: [fileURLToPath(new URL("dedicated-read-release-races.test.js", import.meta.url))],
  },
};
