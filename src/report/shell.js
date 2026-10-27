(function () {
  var nav = document.querySelector("nav.tabs");
  if (!nav) return;
  nav.addEventListener("click", function (e) {
    var btn = e.target.closest("button[data-t]");
    if (!btn) return;
    var buttons = nav.querySelectorAll("button[data-t]");
    for (var i = 0; i < buttons.length; i++) {
      var on = buttons[i] === btn;
      buttons[i].setAttribute("aria-selected", on ? "true" : "false");
      var panel = document.getElementById(buttons[i].getAttribute("data-t") + "-panel");
      if (panel) panel.hidden = !on;
    }
    window.scrollTo(0, 0);
  });
})();
