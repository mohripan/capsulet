import { rm } from "node:fs/promises";
import { AUTH_DIRECTORY } from "./fixture";

/** Saved sessions hold the stack's tokens, so they go when the suite does. */
export default async function globalTeardown() {
  await rm(AUTH_DIRECTORY, { recursive: true, force: true });
}
