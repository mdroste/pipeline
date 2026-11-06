// Keep each equation inside its own bounded box. Fit again after print media
// takes effect: the paper width can differ from the on-screen preview width.
// Changing only the equation's font size preserves its layout and leaves the
// surrounding report at the chosen print size.
(function () {
  const equations = [];
  document.querySelectorAll(".katex").forEach(function (equation) {
    const display = equation.parentElement.classList.contains("katex-display");
    const content = display ? equation.parentElement : equation;
    const box = document.createElement(display ? "div" : "span");
    box.className = "pipeline-math" + (display ? " pipeline-math-display" : "");
    content.replaceWith(box);
    box.appendChild(content);
    equations.push({ box: box, equation: equation });
  });

  function fitEquations() {
    equations.forEach(function (item) {
      item.equation.style.fontSize = "";
    });
    equations.forEach(function (item) {
      const available = item.box.clientWidth;
      const width = item.equation.getBoundingClientRect().width;
      if (available > 0 && width > available) {
        const size = parseFloat(getComputedStyle(item.equation).fontSize);
        // Leave a pixel for fractional layout rounding at the page edge.
        item.equation.style.fontSize = (size * (available - 1)) / width + "px";
      }
    });
  }

  window.addEventListener("beforeprint", fitEquations);
  window.addEventListener("afterprint", fitEquations);
  window.addEventListener("resize", fitEquations);
  const fontsReady = document.fonts ? document.fonts.ready : Promise.resolve();
  fontsReady.then(function () {
    fitEquations();
    requestAnimationFrame(function () {
      window.print();
    });
  });
})();
