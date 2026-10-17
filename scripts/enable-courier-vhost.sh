#!/bin/bash
# Stage + enable Apache vhosts for courier (new rav stack on 127.0.0.1:3002).
# Runs on the droplet as root. Only enables sites when /api/health is green.
set -euo pipefail

cat > /etc/apache2/sites-available/courier.cloudservetechcentral.com.conf <<'EOF'
<VirtualHost *:80>
    ServerName courier.cloudservetechcentral.com
    Redirect permanent / https://courier.cloudservetechcentral.com/
</VirtualHost>
EOF

cat > /etc/apache2/sites-available/courier.cloudservetechcentral.com-le-ssl.conf <<'EOF'
<VirtualHost *:443>
    ServerName courier.cloudservetechcentral.com
    ProxyPreserveHost On
    ProxyRequests Off
    ProxyPass / http://127.0.0.1:3002/
    ProxyPassReverse / http://127.0.0.1:3002/
    RequestHeader set "X-Forwarded-Proto" "https"
    SSLEngine on
    SSLCertificateFile /etc/letsencrypt/live/courier.cloudservetechcentral.com/fullchain.pem
    SSLCertificateKeyFile /etc/letsencrypt/live/courier.cloudservetechcentral.com/privkey.pem
    ErrorLog ${APACHE_LOG_DIR}/courier-error.log
    CustomLog ${APACHE_LOG_DIR}/courier-access.log combined
</VirtualHost>
EOF

echo "== waiting for app health on 3002 =="
for i in $(seq 1 60); do
  if curl -sf --max-time 5 http://127.0.0.1:3002/api/health >/dev/null 2>&1; then
    echo "HEALTH_OK after ${i} checks"
    break
  fi
  [ "$i" = "60" ] && { echo "HEALTH_NEVER_GREEN"; exit 1; }
  sleep 10
done

a2ensite courier.cloudservetechcentral.com courier.cloudservetechcentral.com-le-ssl >/dev/null
apache2ctl configtest
systemctl reload apache2
echo "APACHE_RELOADED"
curl -sk -o /dev/null -w "public https -> %{http_code}\n" --max-time 15 https://courier.cloudservetechcentral.com/
curl -s --max-time 10 https://courier.cloudservetechcentral.com/api/health && echo " PUBLIC_HEALTH_OK"
echo "VHOST_DONE"
