# Build stage for dependencies
FROM composer:2.0 as composer

WORKDIR /app
COPY composer.json composer.lock* ./
RUN composer install --no-scripts --no-autoloader --no-dev

# Final PHP image
FROM php:8.2-fpm

# Install system dependencies
RUN apt-get update && apt-get install -y \
    git \
    curl \
    libpng-dev \
    libonig-dev \
    libxml2-dev \
    libicu-dev \
    zip \
    unzip

# Clear cache
RUN apt-get clean && rm -rf /var/lib/apt/lists/*

# Install PHP extensions
RUN docker-php-ext-install mysqli pdo_mysql mbstring exif pcntl bcmath gd \
    && docker-php-ext-configure intl \
    && docker-php-ext-install intl

# Copy composer binary for potential use in the container
COPY --from=composer /usr/bin/composer /usr/bin/composer

# Set working directory
WORKDIR /var/www

# Create directories for logs and set permissions
RUN mkdir -p /var/log/php-fpm && \
    chown -R www-data:www-data /var/log/php-fpm

# Configure PHP
RUN echo "display_errors = On" >> /usr/local/etc/php/conf.d/error-reporting.ini && \
    echo "error_reporting = E_ALL" >> /usr/local/etc/php/conf.d/error-reporting.ini && \
    echo "log_errors = On" >> /usr/local/etc/php/conf.d/error-reporting.ini && \
    echo "error_log = /var/log/php-fpm/php-errors.log" >> /usr/local/etc/php/conf.d/error-reporting.ini

# Copy application files
COPY --chown=www-data:www-data . /var/www

# Copy vendor directory from composer stage
COPY --from=composer --chown=www-data:www-data /app/vendor /var/www/vendor

# Generate optimized autoloader
RUN composer dump-autoload --optimize

# Set minimal required permissions for vendor directory
RUN find /var/www/vendor -type f -exec chmod 644 {} \; && \
    find /var/www/vendor -type d -exec chmod 755 {} \;

# Expose port 9000 and start php-fpm server
EXPOSE 9000
CMD ["php-fpm"]