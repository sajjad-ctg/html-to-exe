import { render } from "preact";
import App from "./App";

// Suppress the WebView2 right-click menu everywhere except text inputs
// (the title field keeps its paste/reload options).
document.addEventListener("contextmenu", (event) => {
  if (!(event.target instanceof HTMLInputElement)) event.preventDefault();
});

render(<App />, document.getElementById("root"));
