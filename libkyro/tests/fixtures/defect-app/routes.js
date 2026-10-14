/**
 * The route table of the defect fixture application.
 *
 * The same table is used by the server to build the responses and is
 * discovered by Kyro's static route scanner, so the audit and the
 * application agree on what routes exist.
 */
module.exports = [
  { path: "/", title: "Defect App", file: "index.html" },
  { path: "/about", title: "About Defect App", file: "about.html" },
  { path: "/settings", title: "Settings", file: "settings.html" },
  { path: "/reports", title: "Reports", file: "reports.html" },
  { path: "/missing-route", title: "Not Found", file: "notfound.html" },
];
