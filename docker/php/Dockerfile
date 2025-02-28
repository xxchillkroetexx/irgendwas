FROM php:8.2-fpm-alpine

# Install PDO MySQL extension
RUN docker-php-ext-install pdo pdo_mysql

# Keep container lightweight
RUN rm -rf /var/cache/apk/* && \
    rm -rf /tmp/*

WORKDIR /var/www/html

CMD ["php-fpm"]
