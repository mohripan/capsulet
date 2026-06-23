import { NextRequest, NextResponse } from "next/server";

export function proxy(request: NextRequest) {
  if (request.cookies.has("capsulet_session") || process.env.CAPSULET_DASHBOARD_API_TOKEN) {
    return NextResponse.next();
  }
  const login = new URL("/login", request.url);
  login.searchParams.set("next", `${request.nextUrl.pathname}${request.nextUrl.search}`);
  return NextResponse.redirect(login);
}

export const config = {
  matcher: ["/((?!api|login|healthz|_next/static|_next/image|favicon.ico).*)"]
};
