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

# Copy existing application directory permissions
COPY --chown=www-data:www-data . /var/www

# User set in docker-compose.yml, don't set here
# USER www-data

# Expose port 9000 and start php-fpm server
EXPOSE 9000
CMD ["php-fpm"]