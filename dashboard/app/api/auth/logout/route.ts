import { NextRequest, NextResponse } from "next/server";

function secureCookie() {
  if (process.env.CAPSULET_DASHBOARD_COOKIE_SECURE) {
    return process.env.CAPSULET_DASHBOARD_COOKIE_SECURE === "true";
  }
  return (process.env.CAPSULET_DASHBOARD_PUBLIC_URL || "").startsWith("https://");
}

function publicUrl(request: NextRequest) {
  return (process.env.CAPSULET_DASHBOARD_PUBLIC_URL || request.nextUrl.origin).replace(/\/+$/, "");
}

export async function POST(request: NextRequest) {
  const response = NextResponse.redirect(new URL("/login", publicUrl(request)), 303);
  response.cookies.set("capsulet_session", "", {
    httpOnly: true,
    sameSite: "strict",
    secure: secureCookie(),
    path: "/",
    maxAge: 0
  });
  return response;
}
