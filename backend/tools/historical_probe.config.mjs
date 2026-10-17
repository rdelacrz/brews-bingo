// Preserve the archived probe bytes and assertions; adapt only its renamed route.
import base from "../../docs/reviews/evidence/command-lookup-final-return.config.mjs";
import { fileURLToPath } from "node:url";

const probe = fileURLToPath(new URL("../../docs/reviews/evidence/command-lookup-final-return.test.js", import.meta.url));
const oldRoute = '"/api/player-recovery"';

export const historicalRecoveryRoute = {
  name: "historical-player-recovery-route",
  enforce: "pre",
  transform(code, id) {
    if (id.split("?")[0] !== probe) return null;
    if (code.split(oldRoute).length !== 2) throw new Error("Historical recovery route must occur exactly once");
    return { code: code.replace(oldRoute, '"/api/games/player-recovery"'), map: null };
  },
};

export default {
  ...base,
  plugins: [historicalRecoveryRoute, ...base.plugins],
};
