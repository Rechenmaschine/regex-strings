import init, { generate } from "./pkg/regex_strings_demo.js";

const form = document.querySelector("#generator-form");
const patternInput = document.querySelector("#pattern");
const alphabetInput = document.querySelector("#alphabet");
const wordLimitInput = document.querySelector("#word-limit");
const unlimitedInput = document.querySelector("#unlimited");
const maxLengthInput = document.querySelector("#max-length");
const output = document.querySelector("#output");
const count = document.querySelector("#count");
const status = document.querySelector("#status");
const generateButton = document.querySelector("#generate");
const clearButton = document.querySelector("#clear");
const copyButton = document.querySelector("#copy");
const downloadButton = document.querySelector("#download");

let ready = false;
let outputText = "";

function setStatus(message, isError = false) {
  status.textContent = message;
  status.classList.toggle("error", isError);
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

function setOutput(result) {
  outputText = result.text;
  output.value = result.text;
  count.textContent = `${result.count.toLocaleString()} ${result.count === 1 ? "word" : "words"}`;
  copyButton.disabled = !result.text;
  downloadButton.disabled = !result.text;
}

function numberOrNull(input, allowZero = false) {
  const value = input.value.trim();
  if (value === "") return null;
  const number = Number(value);
  if (!Number.isSafeInteger(number) || (allowZero ? number < 0 : number < 1)) {
    throw new Error(`${input.labels[0].textContent} must be a ${allowZero ? "non-negative" : "positive"} whole number.`);
  }
  return number;
}

function syncLimitState() {
  wordLimitInput.disabled = unlimitedInput.checked;
  wordLimitInput.required = !unlimitedInput.checked;
}

unlimitedInput.addEventListener("change", syncLimitState);
syncLimitState();

for (const preset of document.querySelectorAll(".preset")) {
  preset.addEventListener("click", () => {
    alphabetInput.value = preset.dataset.alphabet;
    alphabetInput.focus();
  });
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  if (!ready) return;

  let maxLength;
  let wordLimit;
  try {
    maxLength = numberOrNull(maxLengthInput, true);
    wordLimit = unlimitedInput.checked ? null : numberOrNull(wordLimitInput);
  } catch (error) {
    setStatus(errorMessage(error), true);
    return;
  }
  if (maxLength === null && wordLimit === null) {
    setStatus("Set a max word length when using no limit.", true);
    maxLengthInput.focus();
    return;
  }

  generateButton.disabled = true;
  setStatus("Enumerating…");
  requestAnimationFrame(() => {
    try {
      const result = generate(patternInput.value, alphabetInput.value, maxLength, wordLimit);
      setOutput(result);
      setStatus("Done.");
    } catch (error) {
      setOutput({ text: "", count: 0 });
      setStatus(error instanceof Error ? error.message : String(error), true);
    } finally {
      generateButton.disabled = false;
    }
  });
});

clearButton.addEventListener("click", () => {
  setOutput({ text: "", count: 0 });
  setStatus(ready ? "Ready." : "Loading WebAssembly…");
});

copyButton.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(outputText);
    setStatus("Copied to clipboard.");
  } catch (error) {
    setStatus(`Could not copy output: ${errorMessage(error)}`, true);
  }
});

downloadButton.addEventListener("click", () => {
  const blob = new Blob([outputText], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "regex-strings.txt";
  link.click();
  URL.revokeObjectURL(url);
  setStatus("Downloaded regex-strings.txt.");
});

try {
  await init();
  ready = true;
  generateButton.disabled = false;
  setStatus("Ready.");
} catch (error) {
  setStatus(`WebAssembly could not load: ${error}`, true);
}
