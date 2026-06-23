import { NextRequest, NextResponse } from "next/server";

export const dynamic = "force-dynamic";

function internalIssuerUrl() {
  return (
    process.env.CAPSULET_DASHBOARD_OIDC_INTERNAL_ISSUER ||
    process.env.CAPSULET_DASHBOARD_OIDC_ISSUER ||
    process.env.CAPSULET_DASHBOARD_OIDC_PUBLIC_ISSUER ||
    ""
  ).replace(/\/+$/, "");
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
  const url = new URL(request.url);
  const code = url.searchParams.get("code") || "";
  const state = url.searchParams.get("state") || "";
  const expectedState = request.cookies.get("capsulet_oidc_state")?.value || "";
  const publicBase = browserOrigin(request);
  if (!code || !state || state !== expectedState) {
    return NextResponse.redirect(new URL("/login?error=oidc_state", publicBase), 303);
  }

  const issuer = internalIssuerUrl();
  const clientId = process.env.CAPSULET_DASHBOARD_OIDC_CLIENT_ID || "";
  const clientSecret = process.env.CAPSULET_DASHBOARD_OIDC_CLIENT_SECRET || "";
  if (!issuer || !clientId) {
    return NextResponse.redirect(new URL("/login?error=oidc_config", publicBase), 303);
  }

  const redirectUri = `${browserOrigin(request)}/api/auth/oidc/callback`;
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    code,
    redirect_uri: redirectUri,
    client_id: clientId
  });
  if (clientSecret) {
    body.set("client_secret", clientSecret);
  }

  const tokenResponse = await fetch(`${issuer}/protocol/openid-connect/token`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
    body,
    cache: "no-store"
  }).catch(() => null);
  if (!tokenResponse?.ok) {
    return NextResponse.redirect(new URL("/login?error=oidc_token", publicBase), 303);
  }
  const tokenBody = (await tokenResponse.json()) as { access_token?: string; expires_in?: number };
  if (!tokenBody.access_token) {
    return NextResponse.redirect(new URL("/login?error=oidc_token", publicBase), 303);
  }

  const response = NextResponse.redirect(new URL("/", publicBase), 303);
  response.cookies.set("capsulet_session", tokenBody.access_token, {
    httpOnly: true,
    sameSite: "lax",
    secure: secureCookie(request),
    path: "/",
    maxAge: Math.min(tokenBody.expires_in || 8 * 60 * 60, 8 * 60 * 60)
  });
  response.cookies.set("capsulet_oidc_state", "", {
    httpOnly: true,
    sameSite: "lax",
    secure: secureCookie(request),
    path: "/",
    maxAge: 0
  });
  return response;
}
