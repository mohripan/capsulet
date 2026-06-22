import { NextRequest, NextResponse } from "next/server";

const DEFAULT_API_BASE_URL = "http://127.0.0.1:8080";

export async function POST(request: NextRequest) {
  const body = (await request.json().catch(() => null)) as { token?: unknown } | null;
  const token = typeof body?.token === "string" ? body.token.trim() : "";
  if (!token) {
    return NextResponse.json({ code: "missing_token", message: "Enter an API access token." }, { status: 400 });
  }

  const base = (
    process.env.CAPSULET_DASHBOARD_API_URL ||
    process.env.CAPSULET_API_BASE_URL ||
    DEFAULT_API_BASE_URL
  ).replace(/\/+$/, "");
  const validation = await fetch(`${base}/v1/auth/me`, {
    headers: { authorization: `Bearer ${token}`, accept: "application/json" },
    cache: "no-store"
  }).catch(() => null);
  if (!validation) {
    return NextResponse.json(
      { code: "api_unavailable", message: "The Capsulet API is unavailable." },
      { status: 503 }
    );
  }
  if (!validation.ok) {
    return NextResponse.json(
      { code: "invalid_token", message: "The access token is invalid or expired." },
      { status: 401 }
    );
  }

  const principal = await validation.json();
  const response = NextResponse.json(principal);
  response.cookies.set("capsulet_session", token, {
    httpOnly: true,
    sameSite: "strict",
    secure: process.env.NODE_ENV === "production",
    path: "/",
    maxAge: 8 * 60 * 60
  });
  return response;
}
