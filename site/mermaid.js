// Diagramas: transforma os blocos ```mermaid dos capítulos em desenhos.
// O texto do diagrama fica no próprio .md (dá para ler e versionar); quem
// desenha é a biblioteca Mermaid, carregada só nas páginas que têm diagrama.
// Diagrams: turns the ```mermaid blocks of the chapters into drawings. The
// diagram text stays in the .md itself; the Mermaid library draws it and is
// loaded only on pages that have a diagram.
(function () {
  "use strict";

  var blocos = document.querySelectorAll("pre > code.language-mermaid");
  if (blocos.length === 0) return;

  // Troca cada <pre><code> por um <div> com o texto original guardado,
  // para poder redesenhar quando o leitor trocar o tema claro/escuro.
  var alvos = [];
  blocos.forEach(function (code) {
    var div = document.createElement("div");
    div.className = "diagrama";
    div.style.cssText = "overflow-x:auto;text-align:center;margin:1.5em 0";
    div.dataset.fonte = code.textContent;
    div.textContent = code.textContent;
    code.parentElement.replaceWith(div);
    alvos.push(div);
  });

  function escuro() {
    var c = document.documentElement.classList;
    return c.contains("navy") || c.contains("coal") || c.contains("ayu");
  }

  import("https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.esm.min.mjs")
    .then(function (m) {
      var mermaid = m.default;
      var temaAtual = null;

      function desenhar() {
        var tema = escuro() ? "dark" : "default";
        if (tema === temaAtual) return;
        temaAtual = tema;
        mermaid.initialize({
          startOnLoad: false,
          theme: tema,
          securityLevel: "strict",
          flowchart: { htmlLabels: true, curve: "basis" },
          sequence: { mirrorActors: false },
        });
        alvos.forEach(function (div) {
          div.removeAttribute("data-processed");
          div.textContent = div.dataset.fonte;
        });
        mermaid.run({ nodes: alvos });
      }

      desenhar();
      new MutationObserver(desenhar).observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["class"],
      });
    })
    .catch(function () {
      // Sem internet: o texto do diagrama continua visível, como código.
      alvos.forEach(function (div) {
        div.style.whiteSpace = "pre";
        div.style.fontFamily = "monospace";
      });
    });
})();
