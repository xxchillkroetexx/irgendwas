<?php

/**
 * Global translation functions for easier usage in templates
 */

use SecretSanta\Core\I18n;

if (!function_exists('t')) {
    /**
     * Translate a string
     */
    function t(string $key, array $params = [], string $domain = 'messages'): string
    {
        return I18n::getInstance()->translate($key, $params, $domain);
    }
}

if (!function_exists('format_date')) {
    /**
     * Format a date according to the current locale
     */
    function format_date(\DateTime $date, string $style = 'medium'): string
    {
        return I18n::getInstance()->formatDate($date, $style);
    }
}

if (!function_exists('format_datetime')) {
    /**
     * Format a datetime according to the current locale
     */
    function format_datetime(\DateTime $dateTime, string $dateStyle = 'medium', string $timeStyle = 'short'): string
    {
        return I18n::getInstance()->formatDateTime($dateTime, $dateStyle, $timeStyle);
    }
}

if (!function_exists('format_number')) {
    /**
     * Format a number according to the current locale
     */
    function format_number(float $number, int $decimals = 0): string
    {
        return I18n::getInstance()->formatNumber($number, $decimals);
    }
}

if (!function_exists('format_currency')) {
    /**
     * Format a currency amount according to the current locale
     */
    function format_currency(float $amount, string $currency = 'EUR'): string
    {
        return I18n::getInstance()->formatCurrency($amount, $currency);
    }
}

if (!function_exists('get_available_locales')) {
    /**
     * Get the list of available locales
     */
    function get_available_locales(): array
    {
        return I18n::getInstance()->getAvailableLocales();
    }
}

if (!function_exists('get_current_locale')) {
    /**
     * Get the current locale
     */
    function get_current_locale(): string
    {
        return I18n::getInstance()->getLocale();
    }
}
