"use client";

import { Box, KeyRound, LoaderCircle } from "lucide-react";
import { FormEvent, useState } from "react";

export default function LoginPage() {
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  async function signIn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setSubmitting(true);
    try {
      const response = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ token })
      });
      if (!response.ok) {
        const body = (await response.json()) as { message?: string };
        setError(body.message || "Sign in failed.");
        return;
      }
      window.location.assign("/");
    } catch {
      setError("The dashboard could not reach the Capsulet API.");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <main className="loginShell">
      <section className="loginCard" aria-labelledby="login-title">
        <div className="loginBrand">
          <span className="brandMark"><Box size={22} aria-hidden="true" /></span>
          <div><strong>Capsulet</strong><span>Automation control plane</span></div>
        </div>
        <div className="loginIntro">
          <span className="loginEyebrow"><KeyRound size={15} aria-hidden="true" /> Restricted console</span>
          <h1 id="login-title">Connect with an access token</h1>
          <p>Use a viewer, operator, or administrator token issued by your Capsulet operator.</p>
        </div>
        <form className="loginForm" onSubmit={signIn}>
          <label htmlFor="access-token">Access token</label>
          <input
            autoComplete="current-password"
            id="access-token"
            name="token"
            onChange={(event) => setToken(event.target.value)}
            placeholder="Paste token"
            required
            type="password"
            value={token}
          />
          {error ? <p className="loginError" role="alert">{error}</p> : null}
          <button className="loginButton" disabled={submitting} type="submit">
            {submitting ? <LoaderCircle className="spin" size={17} aria-hidden="true" /> : <KeyRound size={17} aria-hidden="true" />}
            {submitting ? "Connecting…" : "Connect"}
          </button>
        </form>
        <p className="loginFootnote">The token is kept in an HTTP-only browser cookie and is not available to page scripts.</p>
      </section>
    </main>
  );
}
