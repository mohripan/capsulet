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

async function proxy(request: NextRequest, context: { params: { path: string[] } }) {
  const headers = new Headers();
  const contentType = request.headers.get("content-type");
  const accept = request.headers.get("accept");

  if (contentType) {
    headers.set("content-type", contentType);
  }
  if (accept) {
    headers.set("accept", accept);
  }

  const response = await fetch(targetUrl(request, context.params.path), {
    method: request.method,
    headers,
    body: request.method === "GET" || request.method === "HEAD" ? undefined : await request.text(),
    cache: "no-store"
  });

  return new NextResponse(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers
  });
}

export async function GET(request: NextRequest, context: { params: { path: string[] } }) {
  return proxy(request, context);
}

export async function POST(request: NextRequest, context: { params: { path: string[] } }) {
  return proxy(request, context);
}

export async function PUT(request: NextRequest, context: { params: { path: string[] } }) {
  return proxy(request, context);
}

export async function DELETE(request: NextRequest, context: { params: { path: string[] } }) {
  return proxy(request, context);
}
