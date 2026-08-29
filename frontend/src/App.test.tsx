import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("compact dashboard", () => {
  const markup = renderToStaticMarkup(<App />);

  it("labels the OpenAI usage card as Codex", () => {
    expect(markup).toContain(">Codex<");
    expect(markup).not.toContain(">ChatGPT<");
  });

  it("shows provider names without provider icons", () => {
    expect(markup).not.toContain("provider-icon");
  });

  it("does not render the low-value footer", () => {
    expect(markup).not.toContain("<footer");
  });
});
