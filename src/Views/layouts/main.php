<?php
/**
 * Main Layout Template
 * 
 * This is the main layout template that wraps all views in the application.
 * It includes the header, navigation, flash messages, content area, and footer.
 * 
 * @var AuthService $auth Authentication service to check user login status
 * @var User $user Current logged-in user (available through $auth->user())
 * @var SessionService $session Session management service
 * @var string $content The rendered view content to be displayed within this layout
 */
?>
<!DOCTYPE html>
<html lang="en">

<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title><?= t('app.name') ?></title>
    <!-- Include Bootstrap CSS and custom styles -->
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/css/bootstrap.min.css" rel="stylesheet">
    <link rel="stylesheet" href="/assets/css/styles.css">
</head>

<body>
    <header>
        <!-- Main navigation bar -->
        <nav class="navbar navbar-expand-lg navbar-dark bg-primary">
            <div class="container">
                <a class="navbar-brand" href="/"><?= t('app.name') ?></a>
                <!-- Mobile navigation toggle button -->
                <button class="navbar-toggler" type="button" data-bs-toggle="collapse" data-bs-target="#navbarNav">
                    <span class="navbar-toggler-icon"></span>
                </button>
                <div class="collapse navbar-collapse" id="navbarNav">
                    <!-- Main navigation links -->
                    <ul class="navbar-nav me-auto">
                        <?php if ($auth->check()): ?>
                            <li class="nav-item">
                                <a class="nav-link" href="/user/dashboard"><?= t('nav.dashboard') ?></a>
                            </li>
                            <li class="nav-item">
                                <a class="nav-link" href="/groups"><?= t('nav.groups') ?></a>
                            </li>
                        <?php endif; ?>
                    </ul>
                    <!-- User authentication and language options -->
                    <ul class="navbar-nav">
                        <?php if ($auth->check()): ?>
                            <!-- Logged-in user dropdown menu -->
                            <li class="nav-item dropdown">
                                <a class="nav-link dropdown-toggle" href="#" id="userDropdown" data-bs-toggle="dropdown">
                                    <?= htmlspecialchars($auth->user()->getName()) ?>
                                </a>
                                <ul class="dropdown-menu dropdown-menu-end">
                                    <li><a class="dropdown-item" href="/user/profile"><?= t('nav.profile') ?></a></li>
                                    <li>
                                        <hr class="dropdown-divider">
                                    </li>
                                    <li><a class="dropdown-item" href="/auth/logout"><?= t('auth.logout') ?></a></li>
                                </ul>
                            </li>
                        <?php else: ?>
                            <!-- Login/register links for logged-out users -->
                            <li class="nav-item">
                                <a class="nav-link" href="/auth/login"><?= t('auth.login') ?></a>
                            </li>
                            <li class="nav-item">
                                <a class="nav-link" href="/auth/register"><?= t('auth.register') ?></a>
                            </li>
                        <?php endif; ?>
                        <!-- Language selection dropdown -->
                        <li class="nav-item dropdown">
                            <a class="nav-link dropdown-toggle" href="#" id="languageDropdown" data-bs-toggle="dropdown">
                                <?= strtoupper($session->get('locale', 'en')) ?>
                            </a>
                            <ul class="dropdown-menu dropdown-menu-end">
                                <li><a class="dropdown-item" href="/language/en">English</a></li>
                                <li><a class="dropdown-item" href="/language/de">Deutsch</a></li>
                            </ul>
                        </li>
                    </ul>
                </div>
            </div>
        </nav>
    </header>

    <main class="container my-4">
        <!-- Flash messages display section -->
        <?php if ($session->hasFlash('success')): ?>
            <div class="alert alert-success">
                <?= htmlspecialchars($session->getFlash('success')) ?>
            </div>
        <?php endif; ?>

        <?php if ($session->hasFlash('error')): ?>
            <div class="alert alert-danger">
                <?= htmlspecialchars($session->getFlash('error')) ?>
            </div>
        <?php endif; ?>

        <?php if ($session->hasFlash('errors')): ?>
            <div class="alert alert-danger">
                <ul class="mb-0">
                    <?php foreach ($session->getFlash('errors') as $field => $error): ?>
                        <li><?= htmlspecialchars($error) ?></li>
                    <?php endforeach; ?>
                </ul>
            </div>
        <?php endif; ?>

        <!-- Main content area - populated by rendered view -->
        <?= $content ?>
    </main>

    <!-- Page footer -->
    <footer class="mt-auto py-3 bg-light">
        <div class="container text-center">
            <p class="mb-0">&copy; <?= date('Y') ?> <?= t('footer.copyright') ?></p>
        </div>
    </footer>

    <!-- JavaScript libraries -->
    <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/js/bootstrap.bundle.min.js"></script>
    <script src="/assets/js/app.js"></script>
</body>

</html>