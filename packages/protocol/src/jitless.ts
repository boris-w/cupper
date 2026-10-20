// zod probes `new Function` to compile faster parsers, and decides when each
// object schema is built. The renderer's CSP requires Trusted Types for
// scripts, so the probe is a violation there (UI-csp-zero). This module turns
// it off; the entry imports it before the generated schemas.

import { config } from "zod";

config({ jitless: true });
