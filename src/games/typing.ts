import type { GameApi, GameModule } from "../game";

const RUN_SECONDS = 30;

// Short, common words: the test measures typing speed, not vocabulary recall.
const WORDS = [
  "time", "code", "make", "with", "from", "that", "have", "this", "will", "your",
  "look", "then", "them", "some", "into", "over", "just", "know", "take", "than",
  "work", "call", "find", "give", "keep", "left", "life", "line", "list", "load",
  "main", "many", "more", "most", "move", "must", "name", "need", "next", "open",
  "part", "past", "path", "plan", "play", "read", "real", "rest", "root", "same",
  "save", "seem", "send", "show", "side", "site", "size", "sort", "stop", "sure",
  "tell", "term", "test", "text", "true", "turn", "type", "used", "user", "view",
  "wait", "want", "well", "went", "were", "what", "when", "word", "year", "your",
];

function sample(count: number): string[] {
  return Array.from({ length: count }, () => WORDS[Math.floor(Math.random() * WORDS.length)]);
}

/**
 * 30-second typing test. Score is WPM over the elapsed time — so a run cut
 * short by Claude finishing still gives an honest number rather than a
 * partial-minute penalty.
 */
export function createTyping(): GameModule {
  let api: GameApi;
  let wrap: HTMLDivElement;
  let stream: HTMLDivElement;
  let meta: HTMLParagraphElement;
  let words: string[] = sample(120);
  let wordIndex = 0;
  let typed = "";
  let correctChars = 0;
  let typedChars = 0;
  let startedAt: number | null = null;
  let frozen = false;
  let ticker: number | undefined;
  /** Only scroll when advancing words — not on every keystroke. */
  let scrollAtWordIndex = -1;

  function elapsedSeconds(): number {
    if (startedAt === null) return 0;
    return (performance.now() - startedAt) / 1000;
  }

  function wpm(): number {
    const seconds = elapsedSeconds();
    if (seconds < 1) return 0;
    return (correctChars / 5) * (60 / seconds);
  }

  /**
   * Report only once the run is long enough to mean something. Under a second
   * the extrapolation is noise, and recording it as `0 wpm` would install a
   * permanent bogus personal best.
   */
  function reportIfMeaningful(): void {
    if (elapsedSeconds() >= 1 && correctChars > 0) api.reportScore(wpm());
  }

  function accuracy(): number {
    if (typedChars === 0) return 100;
    return (correctChars / typedChars) * 100;
  }

  function render(): void {
    stream.replaceChildren();
    // Keep completed words visible from the start — no sliding window that
    // makes the line jump backward as you progress (monkeytype-style).
    const end = Math.min(words.length, wordIndex + 40);
    for (let i = 0; i < end; i++) {
      const el = document.createElement("span");
      el.className = "word";
      if (i < wordIndex) {
        el.classList.add("done");
        el.textContent = words[i];
      } else if (i === wordIndex) {
        el.classList.add("active");
        const target = words[i];
        // Render the full target up front so the word width stays fixed while typing.
        for (let c = 0; c < target.length; c++) {
          const ch = document.createElement("span");
          const expected = target[c];
          const actual = typed[c];
          if (actual === undefined) {
            ch.textContent = expected;
          } else {
            ch.textContent = expected;
            ch.className = actual === expected ? "hit" : "miss";
          }
          el.appendChild(ch);
        }
        // Extra mistyped characters beyond the word length.
        for (let c = target.length; c < typed.length; c++) {
          const ch = document.createElement("span");
          ch.textContent = typed[c];
          ch.className = "extra";
          el.appendChild(ch);
        }
      } else {
        el.textContent = words[i];
      }
      stream.appendChild(el);
    }

    const left = startedAt === null ? RUN_SECONDS : Math.max(0, RUN_SECONDS - elapsedSeconds());
    meta.textContent =
      startedAt === null
        ? "start typing — 30 seconds"
        : `${left.toFixed(0)}s · ${Math.round(wpm())} wpm · ${accuracy().toFixed(0)}% acc`;

    if (wordIndex !== scrollAtWordIndex) {
      scrollAtWordIndex = wordIndex;
      const active = stream.querySelector(".word.active");
      if (active && wordIndex > 0) {
        active.scrollIntoView({ block: "nearest", inline: "nearest" });
      }
    }
  }

  function commitWord(): void {
    const target = words[wordIndex];
    for (let c = 0; c < Math.max(target.length, typed.length); c++) {
      typedChars += 1;
      if (typed[c] !== undefined && typed[c] === target[c]) correctChars += 1;
    }
    // The space between words counts as a correct keystroke when the word matched.
    if (typed === target) correctChars += 1;
    typedChars += 1;

    wordIndex += 1;
    typed = "";
    if (wordIndex >= words.length - 10) words = words.concat(sample(120));
    reportIfMeaningful();
  }

  function onKey(event: KeyboardEvent): void {
    if (frozen) return;
    if (event.ctrlKey || event.metaKey || event.altKey) return;

    if (event.key === "Backspace") {
      event.preventDefault();
      typed = typed.slice(0, -1);
      render();
      return;
    }
    if (event.key === " ") {
      event.preventDefault();
      if (typed.length > 0) commitWord();
      render();
      return;
    }
    if (event.key.length !== 1) return;

    event.preventDefault();
    if (startedAt === null) {
      startedAt = performance.now();
      ticker = window.setInterval(() => {
        if (elapsedSeconds() >= RUN_SECONDS) finish();
        else render();
      }, 200);
    }
    typed += event.key;
    render();
  }

  function finish(): void {
    if (frozen) return;
    frozen = true;
    window.clearInterval(ticker);
    reportIfMeaningful();
    meta.textContent = `time — ${Math.round(wpm())} wpm · ${accuracy().toFixed(0)}% acc`;
    wrap.dataset.state = "done";
  }

  return {
    id: "typing",
    label: "Typing",
    unit: "wpm",
    lowerIsBetter: false,

    mount(root: HTMLElement, gameApi: GameApi): void {
      api = gameApi;
      wrap = document.createElement("div");
      wrap.className = "typing";
      stream = document.createElement("div");
      stream.className = "typing-stream";
      meta = document.createElement("p");
      meta.className = "typing-meta";
      wrap.append(stream, meta);
      root.appendChild(wrap);

      render();
      window.addEventListener("keydown", onKey);
    },

    freeze(): void {
      if (frozen) return;
      frozen = true;
      window.clearInterval(ticker);
      reportIfMeaningful();
      wrap.dataset.state = "done";
      render();
    },

    destroy(): void {
      frozen = true;
      window.clearInterval(ticker);
      window.removeEventListener("keydown", onKey);
      wrap?.remove();
    },
  };
}
