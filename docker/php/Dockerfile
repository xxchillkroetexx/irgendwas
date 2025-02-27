FROM php:8.2-fpm

# Installiere Abhängigkeiten
RUN apt-get update && apt-get install -y \
    libzip-dev \
    zip \
    unzip \
    && docker-php-ext-install zip pdo pdo_mysql

# Composer installieren
COPY --from=composer:latest /usr/bin/composer /usr/bin/composer

WORKDIR /var/www/html
