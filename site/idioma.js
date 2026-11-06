// Seletor de idioma: o português fica na raiz do site e o inglês em /en/,
// com os mesmos nomes de arquivo. O botão leva para a mesma página no outro
// idioma.
// Language switcher: Portuguese lives at the site root and English under
// /en/, with the same file names. The button opens the same page in the
// other language.
(function () {
  "use strict";

  var caminho = location.pathname;
  var emIngles = /^\/en(\/|$)/.test(caminho);
  var destino = emIngles
    ? caminho.replace(/^\/en(\/|$)/, "/")
    : "/en" + caminho;

  var link = document.createElement("a");
  link.href = destino + location.hash;
  link.hreflang = emIngles ? "pt-BR" : "en";
  link.lang = emIngles ? "pt-BR" : "en";
  link.textContent = emIngles ? "Português" : "English";
  link.title = emIngles ? "Ler esta página em português" : "Read this page in English";
  link.style.cssText =
    "font-size:1.4rem;font-weight:600;text-decoration:none;" +
    "padding:0 .6rem;line-height:var(--menu-bar-height);" +
    "color:var(--icons);white-space:nowrap";

  var direita = document.querySelector(".right-buttons");
  if (direita) direita.insertBefore(link, direita.firstChild);
})();
