// Numeric fields accept simple arithmetic ("1920/2", "72*3+1"), as
// Photoshop's do. A tiny recursive-descent parser; no eval.

export function evalArithmetic(src: string): number {
  const s = src.replace(/\s+/g, "");
  let i = 0;
  const peek = () => s[i];
  function num(): number {
    if (peek() === "(") {
      i++;
      const v = expr();
      if (peek() === ")") i++;
      return v;
    }
    if (peek() === "-") {
      i++;
      return -num();
    }
    if (peek() === "+") {
      i++;
      return num();
    }
    const m = /^\d*\.?\d+(e[-+]?\d+)?|^\d+\.?/i.exec(s.slice(i));
    if (!m) return NaN;
    i += m[0].length;
    return parseFloat(m[0]);
  }
  function term(): number {
    let v = num();
    while (peek() === "*" || peek() === "/") {
      const op = s[i++];
      const r = num();
      v = op === "*" ? v * r : v / r;
    }
    return v;
  }
  function expr(): number {
    let v = term();
    while (peek() === "+" || peek() === "-") {
      const op = s[i++];
      const r = term();
      v = op === "+" ? v + r : v - r;
    }
    return v;
  }
  const v = expr();
  return i === s.length ? v : NaN;
}
