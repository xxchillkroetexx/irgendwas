<!DOCTYPE html>
<html lang="<?= $translator->getLocale() ?>">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title><?= $t('app.name') ?></title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/css/bootstrap.min.css">
    <link rel="stylesheet" href="/assets/css/styles.css">
</head>
<body>
    <header>
        <nav class="navbar navbar-expand-lg navbar-dark bg-primary">
            <div class="container">
                <a class="navbar-brand" href="/"><?= $t('app.name') ?></a>
                <button class="navbar-toggler" type="button" data-bs-toggle="collapse" data-bs-target="#navbarNav" aria-controls="navbarNav" aria-expanded="false" aria-label="Toggle navigation">
                    <span class="navbar-toggler-icon"></span>
                </button>
                <div class="collapse navbar-collapse" id="navbarNav">
                    <ul class="navbar-nav me-auto">
                        <?php if ($auth->check()): ?>
                            <li class="nav-item">
                                <a class="nav-link" href="/user/dashboard"><?= $t('app.dashboard') ?></a>
                            </li>
                            <li class="nav-item">
                                <a class="nav-link" href="/group/list"><?= $t('groups.my_groups') ?></a>
                            </li>
                        <?php endif; ?>
                    </ul>
                    <ul class="navbar-nav">
                        <li class="nav-item dropdown">
                            <a class="nav-link dropdown-toggle" href="#" id="languageDropdown" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                                <?= strtoupper($translator->getLocale()) ?>
                            </a>
                            <ul class="dropdown-menu" aria-labelledby="languageDropdown">
                                <?php foreach ($translator->getAvailableLocales() as $locale): ?>
                                    <li><a class="dropdown-item" href="/language/<?= $locale ?>"><?= strtoupper($locale) ?></a></li>
                                <?php endforeach; ?>
                            </ul>
                        </li>
                        <?php if ($auth->check()): ?>
                            <li class="nav-item dropdown">
                                <a class="nav-link dropdown-toggle" href="#" id="userDropdown" role="button" data-bs-toggle="dropdown" aria-expanded="false">
                                    <?= $view->escape($auth->user()->getName()) ?>
                                </a>
                                <ul class="dropdown-menu" aria-labelledby="userDropdown">
                                    <li><a class="dropdown-item" href="/user/profile"><?= $t('user.profile') ?></a></li>
                                    <li><hr class="dropdown-divider"></li>
                                    <li><a class="dropdown-item" href="/auth/logout"><?= $t('auth.logout') ?></a></li>
                                </ul>
                            </li>
                        <?php else: ?>
                            <li class="nav-item">
                                <a class="nav-link" href="/auth/login"><?= $t('auth.login') ?></a>
                            </li>
                            <li class="nav-item">
                                <a class="nav-link" href="/auth/register"><?= $t('auth.register') ?></a>
                            </li>
                        <?php endif; ?>
                    </ul>
                </div>
            </div>
        </nav>
    </header>

    <main class="container my-4">
        <?php if ($session->hasFlash('success')): ?>
            <div class="alert alert-success">
                <?= $view->escape($session->getFlash('success')) ?>
            </div>
        <?php endif; ?>

        <?php if ($session->hasFlash('error')): ?>
            <div class="alert alert-danger">
                <?= $view->escape($session->getFlash('error')) ?>
            </div>
        <?php endif; ?>

        <?php if ($session->hasFlash('errors')): ?>
            <div class="alert alert-danger">
                <ul class="mb-0">
                    <?php foreach ($session->getFlash('errors') as $field => $fieldErrors): ?>
                        <?php foreach ($fieldErrors as $error): ?>
                            <li><?= $view->escape($error) ?></li>
                        <?php endforeach; ?>
                    <?php endforeach; ?>
                </ul>
            </div>
        <?php endif; ?>

        <?= $content ?>
    </main>

    <footer class="bg-light py-4 mt-5">
        <div class="container text-center">
            <p>&copy; <?= date('Y') ?> <?= $t('app.name') ?></p>
        </div>
    </footer>

    <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/js/bootstrap.bundle.min.js"></script>
    <script src="/assets/js/app.js"></script>
</body>
</html>