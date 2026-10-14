/**
 * A tiny dependency-free static server for the defect fixture application.
 *
 * It prints a readiness line that Kyro's development-server manager can
 * detect, so the audit verifies the URL instead of guessing it.
 */
const http = require("http");
const fs = require("fs");
const path = require("path");

const routes = require("./routes.js");
const PORT = Number(process.env.PORT || 0);
const HOST = "127.0.0.1";

const page = (title, body) => `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>${title}</title>
<style>
  body { font-family: system-ui, sans-serif; margin: 0; padding: 24px; background: #ffffff; color: #171717; }
  nav a { margin-right: 16px; }
  .wide { width: 900px; height: 60px; background: #e5e5e5; }
  .error { color: #b91c1c; }
</style>
</head>
<body>
<nav>
  <a href="/">Home</a>
  <a href="/about">About</a>
  <a href="/settings">Settings</a>
  <a href="/reports">Reports</a>
  <a href="/missing-route">Broken link</a>
</nav>
${body}
</body>
</html>`;

const readPage = (file) => {
  const p = path.join(__dirname, "pages", file);
  if (!fs.existsSync(p)) return null;
  return fs.readFileSync(p, "utf-8");
};

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${HOST}:${PORT}`);
  const route = routes.find((r) => r.path === url.pathname);

  if (!route) {
    res.writeHead(404, { "Content-Type": "text/html; charset=utf-8" });
    res.end(page("Not Found", '<h1>404</h1><p class="error">This page does not exist.</p>'));
    return;
  }

  if (route.path === "/missing-route") {
    // The route exists in the table but the page behind it was never created,
    // so the server answers with a real 404 status.
    res.writeHead(404, { "Content-Type": "text/html; charset=utf-8" });
    res.end(page("Not Found", '<h1>404</h1><p class="error">This page does not exist.</p>'));
    return;
  }

  if (url.pathname === "/assets/missing-logo.png") {
    res.writeHead(404, { "Content-Type": "image/png" });
    res.end();
    return;
  }

  const body = readPage(route.file);
  if (body === null) {
    res.writeHead(404, { "Content-Type": "text/html; charset=utf-8" });
    res.end(page("Not Found", '<h1>404</h1>'));
    return;
  }

  res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
  res.end(page(route.title, body));
});

server.listen(PORT, HOST, () => {
  const actual = server.address().port;
  // The readiness line is what the development-server manager detects.
  console.log(`Defect fixture server ready on http://${HOST}:${actual}/`);
});
