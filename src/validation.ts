export const validMac = (value: string) => /^([0-9a-f]{2}:){5}[0-9a-f]{2}$/i.test(value.trim());

export function validateTarget(bssid: string, client: string, channel: string, count: string): string {
  if (!validMac(bssid)) return "Enter a valid target BSSID (AA:BB:CC:DD:EE:FF).";
  if (client.trim() && !validMac(client)) return "Enter a valid client MAC or leave it empty.";
  const ch = Number(channel);
  if (!Number.isInteger(ch) || ch < 1 || ch > 165) return "Channel must be a whole number from 1 to 165.";
  const packets = Number(count);
  if (!Number.isInteger(packets) || packets < 1 || packets > 9999) return "Packet count must be a whole number from 1 to 9999.";
  return "";
}
