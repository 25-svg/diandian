export function buildSensitiveCommandArgs(
  command: string,
  args: Record<string, any>,
  desktop: boolean,
  idempotencyKey: string,
  traceId: string
): Record<string, any> {
  const confirmationToken = `confirm:${command}`;
  if (desktop) {
    return { ...args, idempotencyKey, confirmationToken, traceId };
  }
  return {
    ...args,
    idempotency_key: idempotencyKey,
    confirmation_token: confirmationToken,
    trace_id: traceId,
  };
}
