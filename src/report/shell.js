(function () {
  var nav = document.querySelector("nav.tabs");
  if (!nav) return;
  var menu = nav.querySelector("button.menu");
  function isOpen() {
    return nav.classList.contains("open");
  }
  function expand(on) {
    if (!menu) return;
    nav.classList.toggle("open", on);
    menu.setAttribute("aria-expanded", on ? "true" : "false");
  }
  // Closing hides the list, and with it whichever tab held focus, so focus
  // goes back to the toggle rather than falling to the body.
  function collapse() {
    if (!isOpen()) return;
    expand(false);
    menu.focus();
  }
  nav.addEventListener("click", function (e) {
    if (menu && e.target.closest("button.menu")) {
      expand(!isOpen());
      return;
    }
    var btn = e.target.closest("button[data-t]");
    if (!btn) return;
    var buttons = nav.querySelectorAll("button[data-t]");
    for (var i = 0; i < buttons.length; i++) {
      var on = buttons[i] === btn;
      buttons[i].setAttribute("aria-selected", on ? "true" : "false");
      var panel = document.getElementById(buttons[i].getAttribute("data-t") + "-panel");
      if (panel) panel.hidden = !on;
    }
    if (menu) menu.querySelector(".cur").textContent = btn.querySelector(".l").textContent;
    collapse();
    window.scrollTo(0, 0);
  });
  // The open menu overlays the page, so a tap anywhere else closes it and
  // only closes it: caught before it reaches the page, it cannot also press
  // whatever was under the overlay's edge.
  document.addEventListener(
    "click",
    function (e) {
      if (!isOpen() || nav.contains(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      expand(false);
    },
    true
  );
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") collapse();
  });
})();
