/**
 * The two documents' CSPs (design 5.7.8), in one place: the shell sends
 * them with the documents, and the screenshot tests render under them
 * (UI-csp-zero), each origin swapped for the browser mock's. The Trusted
 * Types policy the main document names is the one `ui-core` creates; the
 * review document has none, since @pierre/diffs writes strings into its
 * shadow roots, and only the main document may frame it.
 */
export const mainDocumentCsp = (review = "app://cupper-review") =>
  [
    "default-src 'none'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' blob: data:",
    "font-src 'self'",
    "worker-src 'self'",
    "connect-src 'none'",
    `frame-src ${review}`,
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
    "frame-ancestors 'none'",
    "require-trusted-types-for 'script'",
    "trusted-types cupper-worker",
  ].join("; ");

export const reviewDocumentCsp = (main = "app://cupper") =>
  [
    "default-src 'none'",
    "script-src 'self'",
    "worker-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data:",
    "font-src 'self'",
    "connect-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
    `frame-ancestors ${main}`,
  ].join("; ");

export const MAIN_DOCUMENT_CSP = mainDocumentCsp();
export const REVIEW_DOCUMENT_CSP = reviewDocumentCsp();
