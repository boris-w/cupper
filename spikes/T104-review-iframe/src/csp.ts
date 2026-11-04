// The review document's CSP (design 5.7.8), as T131 would keep it beside
// MAIN_DOCUMENT_CSP: no `require-trusted-types-for` (pierre writes strings
// into its shadow roots), and only the main document may frame it.
export const REVIEW_DOCUMENT_CSP = [
  "default-src 'none'",
  "script-src 'self'",
  "worker-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data:",
  "font-src 'self'",
  "connect-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors app://cupper",
].join("; ");
