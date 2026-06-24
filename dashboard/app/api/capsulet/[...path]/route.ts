import { NextRequest, NextResponse } from "next/server";

const DEFAULT_API_BASE_URL = "http://127.0.0.1:8080";

function apiBaseUrl() {
  return (
    process.env.CAPSULET_DASHBOARD_API_URL ||
    process.env.CAPSULET_API_BASE_URL ||
    DEFAULT_API_BASE_URL
  );
}

function targetUrl(request: NextRequest, path: string[]) {
  const base = apiBaseUrl().replace(/\/+$/, "");
  const routePath = path.map(encodeURIComponent).join("/");
  return `${base}/${routePath}${request.nextUrl.search}`;
}

type RouteContext = { params: Promise<{ path: string[] }> };

async function proxy(request: NextRequest, context: RouteContext) {
  const headers = new Headers();
  const contentType = request.headers.get("content-type");
  const accept = request.headers.get("accept");

  if (contentType) {
    headers.set("content-type", contentType);
  }
  if (accept) {
    headers.set("accept", accept);
  }
  const forwardedFor = request.headers.get("x-forwarded-for") || request.headers.get("x-real-ip");
  if (forwardedFor) {
    headers.set("x-forwarded-for", forwardedFor);
  }
  headers.set("accept-encoding", "identity");
  const token = request.cookies.get("capsulet_session")?.value || process.env.CAPSULET_DASHBOARD_API_TOKEN;
  if (token) {
    headers.set("authorization", `Bearer ${token}`);
  }

  const { path } = await context.params;
  const response = await fetch(targetUrl(request, path), {
    method: request.method,
    headers,
    body: request.method === "GET" || request.method === "HEAD" ? undefined : await request.text(),
    cache: "no-store"
  });

  const responseHeaders = new Headers(response.headers);
  responseHeaders.delete("content-encoding");
  responseHeaders.delete("content-length");
  responseHeaders.delete("transfer-encoding");

  return new NextResponse(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: responseHeaders
  });
}

export async function GET(request: NextRequest, context: RouteContext) {
  return proxy(request, context);
}

export async function POST(request: NextRequest, context: RouteContext) {
  return proxy(request, context);
}

export async function PUT(request: NextRequest, context: RouteContext) {
  return proxy(request, context);
}

export async function DELETE(request: NextRequest, context: RouteContext) {
  return proxy(request, context);
}
