import base from "../../../backend/vitest.config.js";
import { fileURLToPath } from "node:url";
export default {
  ...base,
  test: {
    ...base.test,
    include: [fileURLToPath(new URL("./command-lookup-final-return.test.js", import.meta.url))],
  },
};
