import "./style.css";

import {
  DEMO_MINT,
  getAsset,
  getLatestBlockhash,
  getSignaturesForProgram,
  hasApiKey,
  HeliusError,
} from "./helius";

interface DemoCard {
  id: string;
  title: string;
  method: string;
  what: string;
  run: () => Promise<string>;
}

const CARDS: DemoCard[] = [
  {
    id: "blockhash",
    title: "1 · liveness",
    method: "getLatestBlockhash",
    what: "Standard JSON-RPC. Confirms the key works and the chain is reachable; returns the slot and blockhash the node is at.",
    run: async () => {
      const { slot, hash } = await getLatestBlockhash();
      return `slot ${slot.toLocaleString()}\nblockhash ${hash}`;
    },
  },
  {
    id: "sigs",
    title: "2 · recent pump.fun activity",
    method: "DAS getSignaturesForAsset",
    what: `Digital Asset Standard read: the five most recent signatures touching the pump.fun program (${"6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".slice(0, 12)}…).`,
    run: async () => {
      const { total, items } = await getSignaturesForProgram(5);
      if (items.length === 0) {
        return `index reports total: ${total.toLocaleString()} — no signature rows returned for the program id`;
      }
      return items
        .map((item) => `slot ${item.slot.toLocaleString()}  ${item.signature.slice(0, 24)}…${item.err ? "  (failed tx)" : ""}`)
        .join("\n");
    },
  },
  {
    id: "asset",
    title: "3 · one real launch",
    method: "DAS getAsset",
    what: `The full asset record for a real pump.fun mint observed by TrenchWatch on mainnet-beta (${DEMO_MINT.slice(0, 12)}…).`,
    run: async () => {
      const asset = await getAsset(DEMO_MINT);
      return `${asset.name} ($${asset.symbol})\ninterface: ${asset.interface}\nstandard: ${asset.standard}`;
    },
  },
];

function renderKeyStatus(): void {
  const el = document.querySelector<HTMLParagraphElement>("#key-status");
  if (!el) return;
  if (hasApiKey()) {
    el.textContent = "✓ API key loaded from .env.local";
    el.classList.add("ok");
  } else {
    el.textContent = "✗ no key — the buttons below will show the config error state";
    el.classList.add("bad");
  }
}

function renderCards(): void {
  const host = document.querySelector<HTMLElement>("#cards");
  if (!host) return;

  for (const card of CARDS) {
    const section = document.createElement("section");
    section.className = "card";
    section.innerHTML = `
      <h2>${card.title} · <code>${card.method}</code></h2>
      <p>${card.what}</p>
      <button type="button">run</button>
      <pre hidden></pre>
    `;
    const button = section.querySelector("button")!;
    const pre = section.querySelector("pre")!;

    button.addEventListener("click", async () => {
      button.disabled = true;
      button.textContent = "running…";
      pre.hidden = false;
      pre.className = "";
      pre.textContent = "…";
      try {
        pre.textContent = await card.run();
        pre.classList.add("ok");
      } catch (error) {
        if (error instanceof HeliusError) {
          pre.textContent = `[${error.kind}] ${error.message}${error.detail ? `\n${error.detail}` : ""}`;
        } else {
          pre.textContent = `[unexpected] ${error instanceof Error ? error.message : String(error)}`;
        }
        pre.classList.add("bad");
      } finally {
        button.disabled = false;
        button.textContent = "run again";
      }
    });

    host.appendChild(section);
  }
}

renderKeyStatus();
renderCards();
