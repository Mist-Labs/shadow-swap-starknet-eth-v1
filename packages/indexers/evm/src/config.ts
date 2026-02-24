import dotenv from "dotenv";
dotenv.config();

export const config = {
  port: parseInt(process.env.PORT || "3000"),
  nodeEnv: process.env.NODE_ENV || "development",

  // Goldsky webhook secret
  goldskyWebhookSecret: process.env.GOLDSKY_WEBHOOK_SECRET || "",

  // Relayer backend URL
  relayerBaseUrl: process.env.RELAYER_BASE_URL || "http://localhost:8080/api/v1",

  // HMAC secret for authenticating with relayer
  hmacSecret: process.env.HMAC_SECRET || "",
};

// Validation
if (!config.hmacSecret) {
  console.warn("⚠️  HMAC_SECRET not set - relayer authentication will fail");
}

if (!config.goldskyWebhookSecret) {
  console.warn("⚠️  GOLDSKY_WEBHOOK_SECRET not set - webhooks not secured");
}
