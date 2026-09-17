/**
 * Cloudflare Worker for Craft Centralized Version Catalog API
 * Endpoint: https://craft.oguzhanumutlu.com/api/versions.zst
 *
 * Supported operations:
 *   - GET  /api/versions.zst   -> Returns zstandard compressed version catalog
 *   - HEAD /api/versions.zst   -> Headers for cache verification
 *   - PUT  /api/versions.zst   -> Authenticated upload from VDS catalog generator
 *   - GET  /health             -> Health check endpoint
 */

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const path = url.pathname;

    // Handle CORS preflight
    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: corsHeaders(),
      });
    }

    // Health check
    if (path === "/health" || path === "/api/health") {
      return new Response(JSON.stringify({ status: "ok", service: "craft-version-catalog" }), {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          ...corsHeaders(),
        },
      });
    }

    // Version catalog endpoint
    if (path === "/api/versions.zst" || path === "/versions.zst") {
      if (request.method === "GET" || request.method === "HEAD") {
        return handleGetCatalog(request, env, ctx);
      }

      if (request.method === "PUT" || request.method === "POST") {
        return handlePutCatalog(request, env);
      }

      return new Response("Method Not Allowed", {
        status: 405,
        headers: { Allow: "GET, HEAD, PUT, OPTIONS", ...corsHeaders() },
      });
    }

    return new Response("Not Found", { status: 404, headers: corsHeaders() });
  },
};

/**
 * Handle GET /api/versions.zst
 */
async function handleGetCatalog(request, env, ctx) {
  // 1. Try Cloudflare R2 bucket first if bound
  if (env.VERSIONS_BUCKET) {
    const object = await env.VERSIONS_BUCKET.get("versions.zst");
    if (object) {
      const headers = new Headers();
      object.writeHttpMetadata(headers);
      headers.set("etag", object.httpEtag);
      headers.set("Content-Type", "application/zstd");
      headers.set("Content-Disposition", 'attachment; filename="versions.zst"');
      headers.set("Cache-Control", "public, max-age=3600, s-maxage=21600");
      appendCorsHeaders(headers);

      if (request.method === "HEAD") {
        return new Response(null, { headers });
      }
      return new Response(object.body, { headers });
    }
  }

  // 2. Try KV store if bound
  if (env.VERSIONS_KV) {
    const buffer = await env.VERSIONS_KV.get("versions.zst", { type: "arrayBuffer" });
    if (buffer) {
      const headers = new Headers();
      headers.set("Content-Type", "application/zstd");
      headers.set("Content-Disposition", 'attachment; filename="versions.zst"');
      headers.set("Cache-Control", "public, max-age=3600, s-maxage=21600");
      appendCorsHeaders(headers);

      if (request.method === "HEAD") {
        return new Response(null, { headers });
      }
      return new Response(buffer, { headers });
    }
  }

  return new Response("Catalog not found or not yet generated.", {
    status: 404,
    headers: { "Content-Type": "text/plain", ...corsHeaders() },
  });
}

/**
 * Handle authenticated PUT /api/versions.zst from VDS crawler worker
 */
async function handlePutCatalog(request, env) {
  const authHeader = request.headers.get("Authorization") || "";
  const secret = env.CATALOG_UPLOAD_SECRET;

  if (!secret) {
    return new Response("Server error: CATALOG_UPLOAD_SECRET not configured", {
      status: 500,
      headers: corsHeaders(),
    });
  }

  const expectedAuth = `Bearer ${secret}`;
  if (authHeader !== expectedAuth) {
    return new Response("Unauthorized", {
      status: 401,
      headers: corsHeaders(),
    });
  }

  const body = await request.arrayBuffer();
  if (!body || body.byteLength === 0) {
    return new Response("Bad Request: Empty body", {
      status: 400,
      headers: corsHeaders(),
    });
  }

  // Store in R2 if bound
  if (env.VERSIONS_BUCKET) {
    await env.VERSIONS_BUCKET.put("versions.zst", body, {
      httpMetadata: {
        contentType: "application/zstd",
        contentDisposition: 'attachment; filename="versions.zst"',
        cacheControl: "public, max-age=3600, s-maxage=21600",
      },
    });
  }

  // Also store in KV if bound
  if (env.VERSIONS_KV) {
    await env.VERSIONS_KV.put("versions.zst", body);
  }

  return new Response(
    JSON.stringify({
      success: true,
      bytes: body.byteLength,
      timestamp: Date.now(),
    }),
    {
      status: 200,
      headers: {
        "Content-Type": "application/json",
        ...corsHeaders(),
      },
    }
  );
}

function corsHeaders() {
  return {
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET, HEAD, PUT, OPTIONS",
    "Access-Control-Allow-Headers": "Authorization, Content-Type",
  };
}

function appendCorsHeaders(headers) {
  headers.set("Access-Control-Allow-Origin", "*");
  headers.set("Access-Control-Allow-Methods", "GET, HEAD, PUT, OPTIONS");
  headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type");
}
