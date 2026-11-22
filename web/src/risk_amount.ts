export function amount(value: string) {
  const [n, d = "1"] = value.split("/");
  if (!n) return "Unavailable";
  // Round for display after dividing exact integers. Keep the exact value in the title.
  const num = BigInt(n),
    den = BigInt(d),
    abs = num < 0n ? -num : num;
  const scale = 100000000n,
    digits = ((abs * scale + den / 2n) / den).toString().padStart(9, "0");
  if (abs > 0n && abs * scale < den) return value;
  const decimal = digits.slice(-8).replace(/0+$/, "");
  return (
    (num < 0n ? "−" : "") + digits.slice(0, -8) + (decimal ? "." + decimal : "")
  );
}
