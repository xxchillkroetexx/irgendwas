# Deployment mit Docker

Diese Anleitung installiert den **Rust-Rewrite** aus `rust/` auf einem Linux-Server
mit Docker Engine und Docker Compose v2 (`docker compose version`). Ein vorhandener
Reverse Proxy übernimmt HTTPS und Zertifikate. Benötigt werden außerdem eine Domain
und ein erreichbarer SMTP-Server. Die Anwendung wird unter einer eigenen Domain am
Pfad `/` betrieben, nicht unter einem Unterpfad.

Der Stack besteht aus zwei Containern: Anwendung und MariaDB. Das vorhandene
Dockerfile baut Server, Browser-Code und statische Dateien; auf dem Server werden
weder Rust noch Node benötigt. Das Runtime-Image läuft ohne Root-Rechte.
Die Datenbank speichert ihre Daten in einem benannten Docker-Volume und veröffentlicht
keinen Port. HTTP ist ausschließlich unter `127.0.0.1:8080` erreichbar.

## Erstinstallation

Den gewünschten Release-/Branchstand des Repositorys auschecken, dann:

```sh
cd rust
cp deploy.proxy.env.example prod.env
chmod 600 prod.env
```

`prod.env` mit einem Editor anpassen:

| Einstellung | Bedeutung |
| --- | --- |
| `APP_URL` | Öffentliche URL, z. B. `https://wichteln.example`; bestimmt Links, CSRF-Prüfung und sichere Cookies. |
| `DB_PASSWORD`, `DB_ROOT_PASSWORD` | Zwei verschiedene zufällige Passwörter, z. B. jeweils mit `openssl rand -hex 32` erzeugt. Ohne diese Werte verweigert Compose den Start. |
| `MAIL_HOST`, `MAIL_PORT`, `MAIL_ENCRYPTION` | SMTP: üblicherweise `587` mit `tls` (STARTTLS) oder `465` mit `ssl`. |
| `MAIL_USERNAME`, `MAIL_PASSWORD`, `MAIL_FROM_ADDRESS` | SMTP-Zugang und erlaubte Absenderadresse. Registrierung und Passwort-Reset benötigen funktionierenden Mailversand. |
| `APP_LOCALE`, `APP_TIMEZONE` | Standardsprache (`de` oder `en`) und Zeitzone. |
| `INACTIVITY_LIMIT` | Sitzungsdauer bei Inaktivität in Sekunden; voreingestellt sind 1200 (20 Minuten). |
| `HTTP_PORT` | Lokaler HTTP-Port für den vorhandenen Reverse Proxy. |
| `TRUSTED_PROXIES_CIDR` | Vertrauenswürdige Proxy-Adressen; siehe unten. |

Die Datei ist nur lokal und wird von Git und vom Docker-Build ausgeschlossen. Werte
mit `$` oder `#` in einfache Anführungszeichen setzen, damit Compose sie nicht
interpoliert. Immer explizit `--env-file prod.env` verwenden: `.env` ist für Entwicklung.

```sh
docker compose --env-file prod.env -f compose.proxy.yml config --quiet
docker compose --env-file prod.env -f compose.proxy.yml up -d --build --wait --wait-timeout 180
docker compose --env-file prod.env -f compose.proxy.yml ps
curl --fail http://127.0.0.1:8080/health
```

Beim ersten Build werden Toolchain und Abhängigkeiten heruntergeladen; das kann
mehrere Minuten dauern. `CARGO_BUILD_JOBS=2` begrenzt die parallelen Compiler-Prozesse;
bei knappem RAM auf `1` reduzieren. Die Wartezeit oben gilt für den Containerstart.
Die Anwendung startet erst nach erfolgreichem Datenbank-Healthcheck und führt ihre
eingebetteten Migrationen automatisch aus. `/health` muss HTTP 200 mit
`{"database":"ok","status":"ok"}` liefern.

Alternativ ein bereits gebautes Image einsetzen: `APP_IMAGE` auf einen vorhandenen,
festen Tag oder Digest setzen, gegebenenfalls `docker login` ausführen und statt
`--build` diese Befehle verwenden:

```sh
docker compose --env-file prod.env -f compose.proxy.yml pull app mariadb
docker compose --env-file prod.env -f compose.proxy.yml up -d --no-build --wait
```

Ein Feature-Branch erzeugt nicht automatisch ein Registry-Image. Für diesen Branch
funktioniert der lokale Build; die bisherige CI veröffentlicht Images nur für `dev`
und `main`.

## Vorhandenen Reverse Proxy verbinden

Ein Proxy auf demselben Host leitet die komplette Domain an
`http://127.0.0.1:8080` weiter. Er muss `Host` und `Origin` unverändert weitergeben,
HTTP nach HTTPS umleiten und die Client-IP selbst setzen. Beispiel für den
vorhandenen HTTPS-Serverblock eines nginx, der direkt aus dem Internet erreichbar ist:

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host $http_host;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header X-Forwarded-Proto $scheme;
}

add_header Strict-Transport-Security "max-age=31536000" always;
add_header X-Content-Type-Options nosniff always;
add_header X-Frame-Options SAMEORIGIN always;
add_header Referrer-Policy strict-origin-when-cross-origin always;
gzip on;
gzip_types text/css application/javascript application/wasm;
```

TLS-Konfiguration und Zertifikate gehören in den bestehenden Proxy. `Origin` wird
von nginx standardmäßig durchgereicht. Kein Umschreiben auf die interne Adresse:
Anmeldung und Formulare würden sonst an der CSRF-Prüfung scheitern. Der Proxy muss
auch `/pkg/`, `/assets/`, `/api/` und `/health` weiterleiten. Es ist kein zusätzlicher
Caddy im Stack nötig.

Die Anwendung ignoriert `X-Forwarded-For`, solange der unmittelbare Absender nicht
in `TRUSTED_PROXIES_CIDR` steht. Bei rootful Docker und Host-Proxy ist dies normalerweise
das Gateway des eigens angelegten Netzes (`172.31.251.1/32`). Bei abweichendem Netzwerk
`PROXY_SUBNET`, `PROXY_GATEWAY` und die Vertrauensliste gemeinsam anpassen. Keine
pauschale Freigabe mit `0.0.0.0/0`: Nur tatsächlich vorgeschaltete Proxys vertrauen.
Ohne passende Vertrauensliste teilen sich Nutzer die IP-basierten Rate Limits.
Der äußere Proxy muss eingehende Client-IP-Header überschreiben, wie im Beispiel;
bei vorgeschaltetem CDN dessen vertrauenswürdige IP-Auswertung im Proxy konfigurieren.
Die Erkennung von HTTPS in der Anwendung richtet sich immer nach `APP_URL`.

Läuft der vorhandene Proxy selbst in Docker, ist dessen `127.0.0.1` nicht der Host.
Anwendung und Proxy stattdessen an ein gemeinsames externes Netzwerk hängen.
Beispiel `proxy.override.yml` neben `compose.proxy.yml`:

```yaml
services:
  app:
    networks:
      proxy: {}
      database: {}
      existing_proxy: {}
networks:
  existing_proxy:
    external: true
    name: mein-proxy-netz
```

Alle Compose-Befehle dann zusätzlich mit `-f proxy.override.yml` ausführen.
Im Proxy als Ziel `http://app:3000` eintragen (bei mehreren Anwendungen einen
eindeutigen Netzwerkalias konfigurieren) und dessen feste IP im gemeinsamen Netzwerk
als `/32` in `TRUSTED_PROXIES_CIDR` eintragen. Nur der Proxy und die Anwendung gehören
in dieses Netz. Die Datenbank bleibt im internen Netz. Die lokale Portfreigabe wird
dafür nicht benötigt, bleibt aber auf Loopback beschränkt.

Nach Einrichtung `https://<domain>/health` aufrufen und Registrierung einschließlich
Bestätigungsmail, Anmeldung sowie Gruppenerstellung im Browser prüfen. Cookies müssen
bei der öffentlichen HTTPS-URL das Attribut `Secure` haben.

## Betrieb und Updates

Status und letzte Logs:

```sh
docker compose --env-file prod.env -f compose.proxy.yml ps
docker compose --env-file prod.env -f compose.proxy.yml logs --tail=100 app mariadb
```

Vor Updates ein Backup erstellen. Dann den neuen geprüften Git-Stand auschecken und
`up -d --build --wait` wie oben ausführen. Bei Registry-Images erst `APP_IMAGE`
aktualisieren, dann `pull` und `up -d --no-build --wait`. Während des Neustarts kann
die Anwendung kurz nicht erreichbar sein. Eine Änderung von `prod.env` wird mit
`up -d` übernommen; `restart` allein übernimmt keine neue Konfiguration.

`restart: unless-stopped` startet Container nach einem Docker-/Host-Neustart erneut,
sofern Docker beim Booten startet und sie nicht manuell gestoppt wurden. Ein
fehlgeschlagener Healthcheck zeigt einen Fehler an; er bewirkt allein keinen Neustart.

Zum Stoppen `docker compose --env-file prod.env -f compose.proxy.yml down` verwenden.
**Kein `down -v` im produktiven Stack:** Das löscht das Datenbank-Volume.
Den Compose-Projektnamen (`irgendwas`) bei Updates beibehalten, damit dasselbe Volume
verwendet wird. Der bisherige Caddy-Stack mit Projektnamen `irgendwas-rust` hat ein
anderes Volume; ein Wechsel erfordert eine gesicherte Datenübernahme.

## Backup und Wiederherstellung

Diese Befehle in `rust/` ausführen. Backups enthalten persönliche Daten und sollten
geschützt, außerhalb des Hosts aufbewahrt und regelmäßig testweise wiederhergestellt
werden. `prod.env` separat sicher verwahren.

```sh
umask 077
docker compose --env-file prod.env -f compose.proxy.yml exec -T mariadb \
  sh -c 'exec mariadb-dump -uroot -p"$MARIADB_ROOT_PASSWORD" --single-transaction --routines --triggers --databases "$MARIADB_DATABASE"' \
  > backup.sql
```

Nur einen erfolgreich beendeten Dump als Backup verwenden. Wiederherstellung in
einen laufenden Datenbankcontainer (überschreibt vorhandene Daten):

```sh
docker compose --env-file prod.env -f compose.proxy.yml stop app
docker compose --env-file prod.env -f compose.proxy.yml exec -T mariadb \
  sh -c 'exec mariadb -uroot -p"$MARIADB_ROOT_PASSWORD"' < backup.sql
docker compose --env-file prod.env -f compose.proxy.yml up -d --no-build --wait
```

Neue Datenbankpasswörter in `prod.env` ändern keine Benutzer in einem bestehenden
Volume. Dafür die Datenbankbenutzer zuerst gezielt in MariaDB ändern. Ein älteres
App-Image ist nach Schema-Migrationen nicht zwangsläufig kompatibel: Für einen Rollback
den vorherigen Code-/Imagestand und das zugehörige Backup gemeinsam wiederherstellen.

## Fehlerbehebung

- **502 oder unhealthy:** Logs und `/health` prüfen; stimmen Datenbankzugang und
  `APP_URL`? Beim ersten Start ausreichend Zeit für MariaDB und Migrationen lassen.
- **403 bei Formularen:** Öffentliche URL, `Host` und Browser-`Origin` müssen
  zusammenpassen; `APP_URL` darf keine interne HTTP-Adresse sein.
- **Keine E-Mail:** SMTP-Port, Verschlüsselung, Zugang, Absenderfreigabe und Firewall
  prüfen. Ausgehender SMTP-Verkehr muss vom App-Container erlaubt sein.
- **Alle Nutzer treffen dasselbe Rate Limit:** Proxy-IP und `TRUSTED_PROXIES_CIDR`
  prüfen. Rootless Podman kann Quelladressen umschreiben; dessen Besonderheiten
  stehen in [rust/README.md](../rust/README.md#trusted-proxies).
- **Netzwerkbereich belegt:** Freies privates Subnetz samt Gateway in `prod.env`
  wählen und die Proxy-Vertrauensliste entsprechend ändern.

Referenzen: [Docker Compose: Startreihenfolge und Healthchecks](https://docs.docker.com/compose/how-tos/startup-order/),
[nginx: Proxy-Header](https://nginx.org/en/docs/http/ngx_http_proxy_module.html#proxy_set_header).
