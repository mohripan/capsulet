"use client";

import { Box, KeyRound, LoaderCircle, ShieldCheck } from "lucide-react";
import { FormEvent, useState } from "react";

export default function LoginPage() {
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
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
        body: JSON.stringify({ username, password })
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
          <h1 id="login-title">Sign in to Capsulet</h1>
          <p>Use your Capsulet account. Local compose installs include a temporary admin until Keycloak is configured.</p>
        </div>
        <a className="loginButton loginButtonOidc" href="/api/auth/oidc/start">
          <ShieldCheck size={17} aria-hidden="true" />
          Sign in with Keycloak
        </a>
        <div className="loginDivider"><span>or use temporary admin</span></div>
        <form className="loginForm" onSubmit={signIn}>
          <label htmlFor="username">Username</label>
          <input
            autoComplete="username"
            id="username"
            name="username"
            onChange={(event) => setUsername(event.target.value)}
            placeholder="admin"
            required
            type="text"
            value={username}
          />
          <label htmlFor="password">Password</label>
          <input
            autoComplete="current-password"
            id="password"
            name="password"
            onChange={(event) => setPassword(event.target.value)}
            placeholder="Temporary admin password"
            required
            type="password"
            value={password}
          />
          {error ? <p className="loginError" role="alert">{error}</p> : null}
          <button className="loginButton" disabled={submitting} type="submit">
            {submitting ? <LoaderCircle className="spin" size={17} aria-hidden="true" /> : <KeyRound size={17} aria-hidden="true" />}
            {submitting ? "Signing in…" : "Sign in"}
          </button>
        </form>
        <p className="loginFootnote">Sessions are stored in an HTTP-only browser cookie. Raw API tokens are not entered in the dashboard.</p>
      </section>
    </main>
  );
}
