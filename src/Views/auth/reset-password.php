<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('auth.resetPassword.title') ?></h4>
            </div>
            <div class="card-body">
                <form action="/auth/reset-password" method="post">
                    <input type="hidden" name="token" value="<?= htmlspecialchars($token) ?>">

                    <div class="mb-3">
                        <label for="password" class="form-label"><?= t('auth.resetPassword.newPassword') ?></label>
                        <input type="password" class="form-control" id="password" name="password" required minlength="8">
                        <div class="form-text"><?= t('auth.resetPassword.passwordRequirements') ?></div>
                    </div>
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label"><?= t('auth.resetPassword.confirmPassword') ?></label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" required minlength="8">
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('auth.resetPassword.submit') ?></button>
                    </div>
                </form>

                <div class="mt-3 text-center">
                    <a href="/auth/login"><?= t('auth.resetPassword.backToLogin') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>