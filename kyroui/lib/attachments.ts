/**
 * Attachment policy for the production composer (Bug 19).
 *
 * Explicit allowlist + configurable size limits; non-images are classified
 * as text (decoded/verified) or searchable PDFs (routed through the backend
 * document parser) — arbitrary binaries are never read as text.
 */

export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;
export const MAX_TEXT_BYTES = 2 * 1024 * 1024;
export const MAX_PDF_BYTES = 20 * 1024 * 1024;

/** Image MIME types accepted on the binary/base64 path. */
export const IMAGE_MIME_TYPES = ["image/jpeg", "image/png", "image/gif", "image/webp"];

/** Document extensions routed through the backend parser. */
export const PDF_EXTENSIONS = ["pdf"];

/**
 * Text/code extensions decoded as UTF-8 text. Mirror of the backend
 * `DocumentAssistant::detect_format` (non-unsupported, non-pdf set).
 */
export const TEXT_EXTENSIONS = [
  "md", "markdown", "txt", "text", "log", "rst",
  "json", "csv", "tsv",
  "yaml", "yml", "toml", "xml",
  "rs", "ts", "js", "py", "go", "c", "cpp", "java", "sh", "html", "css",
];

export interface FileTypeMeta {
  name: string;
  type: string;
  size: number;
}

export type AttachmentKind = "image" | "pdf" | "text";

export interface AttachmentClassification {
  kind: AttachmentKind | null;
  error: string | null;
}

export function fileExtension(name: string): string {
  const idx = name.lastIndexOf(".");
  return idx >= 0 ? name.slice(idx + 1).toLowerCase() : "";
}

export function classifyAttachment(file: FileTypeMeta): AttachmentClassification {
  const ext = fileExtension(file.name);
  const mime = (file.type || "").toLowerCase();

  if (file.size === 0) {
    return { kind: null, error: `The file "${file.name}" is empty.` };
  }

  // Image family: binary/base64 path.
  const imageExtension = ["jpg", "jpeg", "png", "gif", "webp"].includes(ext);
  const imageMime = mime.startsWith("image/");
  if (imageExtension || imageMime) {
    const validMime = mime === "" || IMAGE_MIME_TYPES.includes(mime);
    if (imageExtension && mime !== "" && !validMime) {
      return {
        kind: null,
        error: `"${file.name}" has an image extension but its content type is "${mime}". A mismatched file cannot be attached as an image.`,
      };
    }
    if (!imageExtension && mime !== "" && !IMAGE_MIME_TYPES.includes(mime)) {
      return {
        kind: null,
        error: `"${file.name}" has unsupported image type "${mime}". Supported images: JPEG, PNG, GIF, WebP.`,
      };
    }
    if (file.size > MAX_IMAGE_BYTES) {
      return {
        kind: null,
        error: `"${file.name}" exceeds the image size limit (${
          MAX_IMAGE_BYTES / (1024 * 1024)
        } MB).`,
      };
    }
    return { kind: "image", error: null };
  }

  // PDF family: backend parser path.
  if (PDF_EXTENSIONS.includes(ext) || mime === "application/pdf") {
    if (ext !== "pdf" && mime === "application/pdf") {
      return {
        kind: null,
        error: `"${file.name}" reports type application/pdf but does not have a .pdf extension; it cannot be parsed as a PDF.`,
      };
    }
    if (file.size > MAX_PDF_BYTES) {
      return {
        kind: null,
        error: `"${file.name}" exceeds the PDF size limit (${
          MAX_PDF_BYTES / (1024 * 1024)
        } MB).`,
      };
    }
    return { kind: "pdf", error: null };
  }

  // Text family: verified UTF-8 decode.
  if (TEXT_EXTENSIONS.includes(ext) || mime.startsWith("text/")) {
    if (!TEXT_EXTENSIONS.includes(ext) && mime.startsWith("text/")) {
      // text/* is accepted; the extension is informational only.
    }
    if (file.size > MAX_TEXT_BYTES) {
      return {
        kind: null,
        error: `"${file.name}" exceeds the text attachment size limit (${
          MAX_TEXT_BYTES / (1024 * 1024)
        } MB).`,
      };
    }
    return { kind: "text", error: null };
  }

  return {
    kind: null,
    error: `"${file.name}" is not a supported attachment type. Supported: images (JPEG, PNG, GIF, WebP), text and code files (${TEXT_EXTENSIONS.join(
      ", "
    )}), and searchable PDFs.`,
  };
}

/** File-picker `accept` attribute for the allowlisted categories. */
export const ACCEPTED_ATTACHMENT_TYPES: string = [
  IMAGE_MIME_TYPES.join(","),
  "image/*",
  ...PDF_EXTENSIONS.map((e) => `.${e}`),
  "application/pdf",
  "text/*",
  ...TEXT_EXTENSIONS.map((e) => `.${e}`),
].join(",");
