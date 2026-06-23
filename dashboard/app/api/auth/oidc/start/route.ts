import { randomUUID } from "crypto";
import { NextRequest, NextResponse } from "next/server";

export const dynamic = "force-dynamic";

function issuerUrl() {
  return process.env.CAPSULET_DASHBOARD_OIDC_PUBLIC_ISSUER || process.env.CAPSULET_DASHBOARD_OIDC_ISSUER || "";
}

function browserOrigin(request: NextRequest) {
  const host = request.headers.get("x-forwarded-host") || request.headers.get("host") || "";
  const protocol = request.headers.get("x-forwarded-proto") || request.nextUrl.protocol.replace(":", "") || "http";
  const origin = host ? `${protocol}://${host}` : request.nextUrl.origin.replace(/\/+$/, "");
  if (!origin.includes("://0.0.0.0")) {
    return origin;
  }
  return (process.env.CAPSULET_DASHBOARD_PUBLIC_URL || origin).replace(/\/+$/, "");
}

function secureCookie(request: NextRequest) {
  if (process.env.CAPSULET_DASHBOARD_COOKIE_SECURE) {
    return process.env.CAPSULET_DASHBOARD_COOKIE_SECURE === "true";
  }
  return browserOrigin(request).startsWith("https://");
}

export async function GET(request: NextRequest) {
  const issuer = issuerUrl().replace(/\/+$/, "");
  const clientId = process.env.CAPSULET_DASHBOARD_OIDC_CLIENT_ID || "";
  if (!issuer || !clientId) {
    return NextResponse.json(
      { code: "oidc_not_configured", message: "Keycloak login is not configured for this dashboard." },
      { status: 404 }
    );
  }

  const state = randomUUID();
  const redirectUri = `${browserOrigin(request)}/api/auth/oidc/callback`;
  const authorize = new URL(`${issuer}/protocol/openid-connect/auth`);
  authorize.searchParams.set("client_id", clientId);
  authorize.searchParams.set("redirect_uri", redirectUri);
  authorize.searchParams.set("response_type", "code");
  authorize.searchParams.set("scope", "openid profile email");
  authorize.searchParams.set("state", state);

  const response = NextResponse.redirect(authorize);
  response.cookies.set("capsulet_oidc_state", state, {
    httpOnly: true,
    sameSite: "lax",
    secure: secureCookie(request),
    path: "/",
    maxAge: 10 * 60
  });
  return response;
}
