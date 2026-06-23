import { NextRequest, NextResponse } from "next/server";

const DEFAULT_API_BASE_URL = "http://127.0.0.1:8080";

function secureCookie() {
  if (process.env.CAPSULET_DASHBOARD_COOKIE_SECURE) {
    return process.env.CAPSULET_DASHBOARD_COOKIE_SECURE === "true";
  }
  return (process.env.CAPSULET_DASHBOARD_PUBLIC_URL || "").startsWith("https://");
}

export async function POST(request: NextRequest) {
  const body = (await request.json().catch(() => null)) as {
    username?: unknown;
    password?: unknown;
  } | null;
  const username = typeof body?.username === "string" ? body.username.trim() : "";
  const password = typeof body?.password === "string" ? body.password : "";
  const expectedUsername = process.env.CAPSULET_TEMP_ADMIN_USERNAME || "admin";
  const expectedPassword = process.env.CAPSULET_TEMP_ADMIN_PASSWORD || "";
  const token =
    process.env.CAPSULET_TEMP_ADMIN_API_TOKEN ||
    process.env.CAPSULET_DASHBOARD_API_TOKEN ||
    "";
  if (!username || !password) {
    return NextResponse.json({ code: "missing_credentials", message: "Enter a username and password." }, { status: 400 });
  }
  if (!expectedPassword || username !== expectedUsername || password !== expectedPassword || !token) {
    return NextResponse.json(
      { code: "invalid_credentials", message: "The username or password is incorrect." },
      { status: 401 }
    );
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
    secure: secureCookie(),
    path: "/",
    maxAge: 8 * 60 * 60
  });
  return response;
}
