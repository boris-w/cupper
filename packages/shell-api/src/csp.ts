/**
 * The main document's CSP (design 5.7.8), in one place: the shell sends it
 * with the document, and the screenshot tests render under it (UI-csp-zero).
 * The Trusted Types policy it names is the one `ui-core` creates.
 */
export const MAIN_DOCUMENT_CSP = [
  "default-src 'none'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' blob: data:",
  "font-src 'self'",
  "worker-src 'self'",
  "connect-src 'none'",
  "frame-src app://cupper-review",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
  "require-trusted-types-for 'script'",
  "trusted-types cupper-worker",
].join("; ");
