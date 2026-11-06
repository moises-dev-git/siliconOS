// Comentários do site (Giscus): cada página ganha um tópico nas Discussions
// de github.com/moises-dev-git/siliconOS, na categoria "Announcements"
// (só o dono cria tópicos; os visitantes comentam nos que o Giscus abre).
(function () {
  "use strict";

  // A página 404 não recebe comentários.
  if (document.title.indexOf("Page not found") === 0) return;

  var main = document.querySelector("main");
  if (!main) return;

  // Identificador estável de cada página: o caminho sem ".html" e sem a barra
  // final. Assim "/03-gdt-e-tss.html" (local) e "/03-gdt-e-tss" (Cloudflare)
  // caem no mesmo tópico, e a página inicial vira "index".
  var termo = location.pathname
    .replace(/\.html$/, "")
    .replace(/\/index$/, "/")
    .replace(/^\/+|\/+$/g, "") || "index";

  // Temas escuros do mdBook → tema escuro do Giscus.
  var ESCUROS = ["coal", "navy", "ayu"];
  function temaGiscus() {
    var classes = document.documentElement.classList;
    for (var i = 0; i < ESCUROS.length; i++) {
      if (classes.contains(ESCUROS[i])) return "dark";
    }
    return "light";
  }

  var secao = document.createElement("section");
  secao.style.marginTop = "4rem";
  secao.innerHTML = '<hr><h2 id="comentarios">Comentários</h2>';
  main.appendChild(secao);

  var s = document.createElement("script");
  s.src = "https://giscus.app/client.js";
  s.async = true;
  s.crossOrigin = "anonymous";
  var config = {
    "data-repo": "moises-dev-git/siliconOS",
    "data-repo-id": "R_kgDOVkoqQg",
    "data-category": "Announcements",
    "data-category-id": "DIC_kwDOVkoqQs4DKfUS",
    "data-mapping": "specific",
    "data-term": termo,
    "data-strict": "1",
    "data-reactions-enabled": "1",
    "data-emit-metadata": "0",
    "data-input-position": "top",
    "data-theme": temaGiscus(),
    "data-lang": "pt",
    "data-loading": "lazy"
  };
  for (var k in config) s.setAttribute(k, config[k]);
  secao.appendChild(s);

  // Quando o leitor troca o tema do mdBook (ícone do pincel), avisa o Giscus.
  new MutationObserver(function () {
    var iframe = document.querySelector("iframe.giscus-frame");
    if (!iframe) return;
    iframe.contentWindow.postMessage(
      { giscus: { setConfig: { theme: temaGiscus() } } },
      "https://giscus.app"
    );
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
})();
