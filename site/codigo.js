// Transforma os nomes do código (funções, tipos, statics, macros) em links
// para a linha onde estão definidos no GitHub. Os dados vêm de simbolos.js,
// gerado por simbolos.py a cada publicação.
// Turns names in code (functions, types, statics, macros) into links to the
// line where they are defined on GitHub. Data comes from simbolos.js.
(function () {
  "use strict";
  var S = window.SIMBOLOS;
  if (!S) return;
  var emIngles = /^\/en(\/|$)/.test(location.pathname);
  var DICA = emIngles ? "See the definition on GitHub: " : "Ver a definição no GitHub: ";
  var ESPECIAIS = { self: 1, Self: 1, crate: 1, super: 1 };
  // Nomes que, soltos no texto, quase sempre falam da biblioteca padrão do
  // Rust (ou de outra linguagem), não do nosso código: não viram link ali.
  var PADRAO = { println: 1, print: 1, eprintln: 1, format: 1, vec: 1, write: 1,
                 writeln: 1, assert: 1, assert_eq: 1, panic: 1, main: 1 };

  var estilo = document.createElement("style");
  estilo.textContent =
    "a.link-codigo{color:inherit;text-decoration:underline dotted;" +
    "text-decoration-color:currentColor;text-underline-offset:2px}" +
    "a.link-codigo:hover{text-decoration-style:solid}";
  document.head.appendChild(estilo);

  // Onde mora `partes` (["Serial","init"]); `depois` é o 1º caractere após o
  // nome e `antes` o que vem antes dele. Devolve "arquivo#Lnn" ou nada.
  function resolver(partes, antes, depois, deInline) {
    var nome = partes[partes.length - 1];
    if (partes.length >= 2) {
      var dono = partes[partes.length - 2];
      if (!ESPECIAIS[dono]) return S.q[dono + "::" + nome];
    }
    if (antes === ".") return depois === "(" ? S.m[nome] : undefined;
    var maiusculo = /^[A-Z]/.test(nome);
    if (depois === "(" || depois === "!" || maiusculo) return S.s[nome];
    // Num `código` do texto, aceita o nome solto se tiver "_" (kernel_main),
    // para não ligar palavras comuns como `ler` ou `sair`.
    if (deInline && nome.indexOf("_") >= 0) return S.s[nome];
  }

  function link(alvo) {
    var a = document.createElement("a");
    a.href = S.base + alvo;
    a.className = "link-codigo";
    a.target = "_blank";
    a.rel = "noopener";
    a.title = DICA + alvo.replace("#L", ":");
    return a;
  }

  var NOME = /[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*/g;

  // Blocos de código Rust: procura os nomes no texto inteiro do bloco (o
  // destaque de sintaxe quebra `Serial::init` em vários pedaços) e embrulha
  // cada parte que tem definição conhecida.
  function bloco(code) {
    var nos = [], texto = "";
    var andar = document.createTreeWalker(code, NodeFilter.SHOW_TEXT);
    while (andar.nextNode()) {
      var no = andar.currentNode;
      var fora = no.parentNode.closest(".hljs-comment, .hljs-string, .hljs-meta, a");
      nos.push({ no: no, ini: texto.length, fora: !!fora });
      texto += no.nodeValue;
    }
    var trocas = [], m;
    NOME.lastIndex = 0;
    while ((m = NOME.exec(texto))) {
      var antes = texto[m.index - 1] || "";
      if (antes === "'" || /\d/.test(m[0][0])) continue;
      var resto = texto.slice(m.index + m[0].length);
      var depois = (resto.match(/^\s*([(!])/) || [])[1] || "";
      var partes = m[0].split("::");
      var pos = m.index;
      for (var i = 0; i < partes.length; i++) {
        // A última parte é o nome procurado; antes dela, só o tipo do começo
        // (o `Serial` de `Serial::init`) também vira link.
        var alvo = i === partes.length - 1
          ? resolver(partes, antes, depois, false)
          : (i === 0 && /^[A-Z]/.test(partes[0]) ? S.s[partes[0]] : undefined);
        if (alvo) trocas.push({ ini: pos, fim: pos + partes[i].length, alvo: alvo });
        pos += partes[i].length + 2;
      }
    }
    // Aplica de trás para frente, para não mexer nas posições ainda não usadas.
    for (var t = trocas.length - 1; t >= 0; t--) {
      var tr = trocas[t];
      for (var k = nos.length - 1; k >= 0; k--) {
        var n = nos[k];
        if (n.ini > tr.ini) continue;
        var fim = n.ini + n.no.nodeValue.length;
        if (n.fora || tr.fim > fim) break;   // em comentário/string, ou partido: deixa
        var meio = n.no.splitText(tr.ini - n.ini);
        meio.splitText(tr.fim - tr.ini);
        var a = link(tr.alvo);
        meio.parentNode.replaceChild(a, meio);
        a.appendChild(meio);
        break;
      }
    }
  }

  // `código` no meio do texto: liga o elemento inteiro se ele for só um nome,
  // com ou sem () ou ! no fim: `Serial::init()`, `kernel_main`, `CONSOLE`.
  function inline(code) {
    if (code.closest("a, h1, h2, h3, h4, h5, h6")) return;
    var m = code.textContent.trim().match(/^([A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)(\(.*\)|!)?$/);
    if (!m) return;
    var depois = m[2] ? m[2][0] : "";
    if (m[1].indexOf("::") < 0 && PADRAO[m[1]]) return;
    var alvo = resolver(m[1].split("::"), "", depois, true);
    if (!alvo) return;
    var a = link(alvo);
    code.parentNode.replaceChild(a, code);
    a.appendChild(code);
  }

  function aplicar() {
    var main = document.querySelector("main") || document.body;
    main.querySelectorAll("pre > code.language-rust").forEach(bloco);
    main.querySelectorAll("code").forEach(function (c) {
      if (!c.closest("pre")) inline(c);
    });
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", aplicar);
  else aplicar();
})();
