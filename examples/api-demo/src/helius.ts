/**
 * Thin, honest wrappers over Helius. One fetch helper carries every error
 * state to the caller: network failure, non-200, JSON-RPC error body, and
 * timeout. Nothing here retries silently or fakes a result.
 */

const API_KEY: string = import.meta.env.VITE_HELIUS_API_KEY ?? "";
const BASE = "https://mainnet.helius-rpc.com";

export function hasApiKey(): boolean {
  return API_KEY.trim().length > 0;
}

export class HeliusError extends Error {
  constructor(
    message: string,
    readonly kind: "config" | "http" | "rpc" | "network" | "timeout",
    readonly detail?: string,
  ) {
    super(message);
    this.name = "HeliusError";
  }
}

async function rpcCall(method: string, params: unknown): Promise<unknown> {
  if (!hasApiKey()) {
    throw new HeliusError(
      "no API key — copy .env.example to .env.local and add VITE_HELIUS_API_KEY",
      "config",
    );
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);

  let response: Response;
  try {
    response = await fetch(`${BASE}/?api-key=${API_KEY.trim()}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
      signal: controller.signal,
    });
  } catch (error) {
    clearTimeout(timer);
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new HeliusError("request timed out after 10s", "timeout");
    }
    throw new HeliusError(
      `network failure: ${error instanceof Error ? error.message : String(error)}`,
      "network",
    );
  }
  clearTimeout(timer);

  if (!response.ok) {
    const body = await response.text().catch(() => "<unreadable body>");
    throw new HeliusError(
      `HTTP ${response.status} from mainnet.helius-rpc.com`,
      "http",
      body.slice(0, 500),
    );
  }

  const payload = (await response.json()) as {
    result?: unknown;
    error?: { code: number; message: string };
  };
  if (payload.error) {
    throw new HeliusError(
      `JSON-RPC error ${payload.error.code}: ${payload.error.message}`,
      "rpc",
    );
  }
  return payload.result;
}

/** 1. Cheapest possible liveness check — a standard JSON-RPC method. */
export async function getLatestBlockhash(): Promise<{ slot: number; hash: string }> {
  const result = (await rpcCall("getLatestBlockhash", [])) as {
    context: { slot: number };
    value: { blockhash: string };
  };
  return { slot: result.context.slot, hash: result.value.blockhash };
}

/** The pump.fun program — every launch on the curve lives under this id. */
export const PUMP_FUN_PROGRAM = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

/**
 * 2. DAS read: signatures that touched one program. This is how you ask
 *    "what happened on pump.fun recently?" without running an indexer.
 */
export async function getSignaturesForProgram(limit = 5): Promise<{
  total: number;
  items: { signature: string; slot: number; err: boolean }[];
}> {
  const result = (await rpcCall("getSignaturesForAsset", {
    id: PUMP_FUN_PROGRAM,
    limit,
    sortDirection: "desc",
  })) as {
    total: number;
    items: { signature: string; slot: number; err: unknown }[];
  };
  return {
    total: result.total,
    items: result.items.map((item) => ({
      signature: item.signature,
      slot: item.slot,
      err: item.err !== null,
    })),
  };
}

/**
 * 3. DAS read: the full asset record for one real pump.fun mint. The demo
 *    mint below was launched on mainnet-beta and observed by TrenchWatch.
 */
export const DEMO_MINT = "13GnNzG34LbfGXSSNYjK7ZWjd1MKywKX8Xvq5PtQkJmR";

export async function getAsset(mint: string): Promise<{
  name: string;
  symbol: string;
  interface: string;
  standard: string;
}> {
  const result = (await rpcCall("getAsset", [{ id: mint }])) as {
    content?: {
      metadata?: { name?: string; symbol?: string };
      links?: { image?: string };
    };
    interface?: string;
    standard?: string;
  };
  return {
    name: result.content?.metadata?.name ?? "(unnamed)",
    symbol: result.content?.metadata?.symbol ?? "???",
    interface: result.interface ?? "(none reported)",
    standard: result.standard ?? "(none reported)",
  };
}
