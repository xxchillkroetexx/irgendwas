<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('auth.forgotPassword.title') ?></h4>
            </div>
            <div class="card-body">
                <p class="mb-3"><?= t('auth.forgotPassword.description') ?></p>

                <form action="/auth/forgot-password" method="post">
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= t('auth.forgotPassword.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required>
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('auth.forgotPassword.submit') ?></button>
                    </div>
                </form>

                <div class="mt-3 text-center">
                    <a href="/auth/login"><?= t('auth.forgotPassword.backToLogin') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>