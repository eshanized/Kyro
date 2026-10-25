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

/**
 * Strict UTF-8 verification for file attachments (Bug 19).
 * Decodes the array buffer with fatal=true so invalid UTF-8 sequences throw
 * rather than being silently coerced to Unicode replacement characters (U+FFFD).
 */
export function validateStrictUtf8(buffer: ArrayBuffer): string {
  const decoder = new TextDecoder("utf-8", { fatal: true });
  return decoder.decode(buffer);
}

/**
 * Checks for known binary file signatures / executable magic bytes (Bug 19).
 * Returns true if the buffer matches ELF, PE/DOS, Mach-O, ZIP/JAR, WebAssembly,
 * or contains null bytes in the header.
 */
export function detectBinaryMagicBytes(buffer: ArrayBuffer): boolean {
  const bytes = new Uint8Array(buffer);
  if (bytes.length < 2) return false;

  // ELF executable: \x7fELF
  if (bytes.length >= 4 && bytes[0] === 0x7f && bytes[1] === 0x45 && bytes[2] === 0x4c && bytes[3] === 0x46) {
    return true;
  }

  // PE / DOS executable: MZ
  if (bytes[0] === 0x4d && bytes[1] === 0x5a) {
    return true;
  }

  // Mach-O binaries: 0xfeedface, 0xfeedfacf, 0xcafebabe, 0xcefaedfe, 0xcffaedfe
  if (bytes.length >= 4) {
    const magic = (bytes[0] << 24) | (bytes[1] << 16) | (bytes[2] << 8) | bytes[3];
    if (
      magic === 0xfeedface ||
      magic === 0xfeedfacf ||
      magic === 0xcefaedfe ||
      magic === 0xcffaedfe ||
      magic === 0xcafebabe
    ) {
      return true;
    }
  }

  // WebAssembly binary: \0asm (0x00, 0x61, 0x73, 0x6d)
  if (bytes.length >= 4 && bytes[0] === 0x00 && bytes[1] === 0x61 && bytes[2] === 0x73 && bytes[3] === 0x6d) {
    return true;
  }

  // ZIP / Jar / DOCX / APK: PK\x03\x04
  if (bytes.length >= 4 && bytes[0] === 0x50 && bytes[1] === 0x4b && bytes[2] === 0x03 && bytes[3] === 0x04) {
    return true;
  }

  // Check for null bytes in initial sample (first 4096 bytes)
  const sampleLen = Math.min(bytes.length, 4096);
  for (let i = 0; i < sampleLen; i++) {
    if (bytes[i] === 0x00) {
      return true;
    }
  }

  return false;
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
