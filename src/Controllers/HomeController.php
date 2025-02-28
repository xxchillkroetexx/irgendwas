<?php

namespace SecretSanta\Controllers;

class HomeController {
    public function index() {
        return '<!DOCTYPE html>
        <html>
        <head>
            <title>Secret Santa</title>
            <style>
                body {
                    font-family: Arial, sans-serif;
                    max-width: 800px;
                    margin: 0 auto;
                    padding: 20px;
                }
                h1 {
                    color: #336699;
                }
                .success {
                    color: green;
                }
            </style>
        </head>
        <body>
            <h1>Secret Santa Web Application</h1>
            <p class="success">✅ Application is working correctly!</p>
            <p>This is the home page of the Secret Santa application.</p>
        </body>
        </html>';
    }
    
    public function setLanguage($locale) {
        return "Language set to: " . $locale;
    }
}