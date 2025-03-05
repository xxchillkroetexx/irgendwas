FROM php:8.2-fpm

# Install system dependencies
RUN apt-get update && apt-get install -y \
    git \
    curl \
    libpng-dev \
    libonig-dev \
    libxml2-dev \
    zip \
    unzip

# Clear cache
RUN apt-get clean && rm -rf /var/lib/apt/lists/*

# Install PHP extensions
RUN docker-php-ext-install pdo_mysql mbstring exif pcntl bcmath gd

# Install Composer
COPY --from=composer:latest /usr/bin/composer /usr/bin/composer

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

# Copy composer files first
COPY composer.json composer.lock* ./

# Install dependencies
RUN composer install --no-scripts

# Copy existing application directory permissions
COPY --chown=www-data:www-data . /var/www

# Set more restricted permissions for vendor directory
RUN find /var/www/vendor -type f -exec chmod 400 {} \; && \
    find /var/www/vendor -type d -exec chmod 500 {} \;

# Expose port 9000 and start php-fpm server
EXPOSE 9000
CMD ["php-fpm"]