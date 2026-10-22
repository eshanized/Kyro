import "@testing-library/jest-dom/vitest";

// jsdom does not implement `scrollIntoView`, which the message thread uses
// to keep the newest content in view.  The stub keeps the tests free of
// "not a function" errors without changing any behaviour under test.
if (typeof Element !== "undefined" && !Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = function scrollIntoView() {};
}

// jsdom does not implement `window.matchMedia`, needed by xterm and responsive components
if (typeof window !== "undefined" && !window.matchMedia) {
  window.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }) as any;
}
