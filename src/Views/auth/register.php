<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-success text-white">
                <h4 class="mb-0"><?= t('auth.register.title') ?></h4>
            </div>
            <div class="card-body">
                <form action="/auth/register" method="post">
                    <div class="mb-3">
                        <label for="name" class="form-label"><?= t('auth.register.name') ?></label>
                        <input type="text" class="form-control" id="name" name="name" required
                            value="<?= htmlspecialchars($session->getFlash('old_input')['name'] ?? '') ?>">
                    </div>
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= t('auth.register.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required
                            value="<?= htmlspecialchars($session->getFlash('old_input')['email'] ?? '') ?>">
                    </div>
                    <div class="mb-3">
                        <label for="password" class="form-label"><?= t('auth.register.password') ?></label>
                        <input type="password" class="form-control" id="password" name="password" required minlength="8">
                        <div class="form-text"><?= t('auth.register.passwordRequirements') ?></div>
                    </div>
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label"><?= t('auth.register.confirmPassword') ?></label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" required minlength="8">
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-success"><?= t('auth.register.submit') ?></button>
                    </div>
                </form>

                <hr>

                <div class="text-center">
                    <p><?= t('auth.register.haveAccount') ?></p>
                    <a href="/auth/login" class="btn btn-outline-primary"><?= t('auth.login') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>