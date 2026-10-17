#!/bin/bash
# Deploy the new rav/courier stack: extract, env, compose build + up.
# Runs on the droplet as root.
set -euo pipefail
cd /var/www/courier

echo "== EXTRACT =="
rm -rf backend frontend Dockerfile docker-compose.yml AGENTS.md LICENSE .git 2>/dev/null || true
tar xzf /root/courier-new-src.tgz
ls

echo "== ENV =="
cat > /var/www/courier/.env <<'EOF'
IMAP_HOST=mail.cloudservetechcentral.com
IMAP_PORT=993
SMTP_HOST=mail.cloudservetechcentral.com
SMTP_PORT=587
TLS_ENABLED=true
RUST_LOG=info,tantivy=warn,async_imap=warn
SESSION_TIMEOUT_HOURS=24
EOF
chmod 600 /var/www/courier/.env
echo "env written"

echo "== PROD COMPOSE =="
cat > /var/www/courier/docker-compose.prod.yml <<'EOF'
services:
  app:
    build:
      context: .
    image: courier-app:local
    container_name: courier-app
    restart: unless-stopped
    ports:
      - "127.0.0.1:3002:3001"
    env_file:
      - .env
    volumes:
      - rav-data:/data

volumes:
  rav-data:
EOF
echo "compose written"

echo "== BUILD (this is the long part) =="
docker compose -f docker-compose.prod.yml build --progress=plain 2>&1 | tail -25
echo "BUILD_EXIT=$?"

echo "== UP =="
docker compose -f docker-compose.prod.yml up -d
sleep 8
echo "== HEALTH =="
curl -sf --max-time 10 http://127.0.0.1:3002/api/health && echo " HEALTH_OK" || { echo "HEALTH_FAILED"; docker logs courier-app 2>&1 | tail -20; }
docker ps --format "{{.Names}} | {{.Status}}" | grep courier
echo "DEPLOY_SCRIPT_DONE"
