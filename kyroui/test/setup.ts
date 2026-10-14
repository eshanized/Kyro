import "@testing-library/jest-dom/vitest";

// jsdom does not implement `scrollIntoView`, which the message thread uses
// to keep the newest content in view.  The stub keeps the tests free of
// "not a function" errors without changing any behaviour under test.
if (typeof Element !== "undefined" && !Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = function scrollIntoView() {};
}
