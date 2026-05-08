import { NextRequest } from "next/server";

export const dynamic = "force-dynamic";

const RUST_API_BASE =
  process.env.ASSET_AGENT_API_URL ??
  "http://127.0.0.1:8080";
const RUST_API_TOKEN = process.env.ASSET_AGENT_API_TOKEN;

type RouteContext = {
  params: Promise<{
    path: string[];
  }>;
};

export async function GET(request: NextRequest, context: RouteContext) {
  return proxyToRustApi(request, context);
}

export async function POST(request: NextRequest, context: RouteContext) {
  return proxyToRustApi(request, context);
}

async function proxyToRustApi(request: NextRequest, context: RouteContext) {
  const { path } = await context.params;
  const target = new URL(`/${path.join("/")}${request.nextUrl.search}`, RUST_API_BASE);
  const headers = forwardHeaders(request.headers);
  const body = hasBody(request.method) ? await request.text() : undefined;

  const response = await fetch(target, {
    body,
    cache: "no-store",
    headers,
    method: request.method,
    redirect: "manual",
  });

  return new Response(response.body, {
    headers: responseHeaders(response.headers),
    status: response.status,
    statusText: response.statusText,
  });
}

function hasBody(method: string) {
  return !["GET", "HEAD"].includes(method);
}

function forwardHeaders(headers: Headers) {
  const forwarded = new Headers();
  const contentType = headers.get("content-type");
  const accept = headers.get("accept");

  if (contentType) {
    forwarded.set("content-type", contentType);
  }
  if (accept) {
    forwarded.set("accept", accept);
  }
  if (RUST_API_TOKEN) {
    forwarded.set("authorization", `Bearer ${RUST_API_TOKEN}`);
  }
  return forwarded;
}

function responseHeaders(headers: Headers) {
  const forwarded = new Headers(headers);
  forwarded.delete("connection");
  forwarded.delete("content-encoding");
  forwarded.delete("content-length");
  forwarded.delete("keep-alive");
  forwarded.delete("transfer-encoding");
  return forwarded;
}
