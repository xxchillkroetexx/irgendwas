<div class="row justify-content-center">
    <div class="col-md-8">
        <div class="card mb-4">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('user.profile.title') ?></h4>
            </div>
            <div class="card-body">
                <form action="/user/profile" method="post">
                    <div class="mb-3">
                        <label for="name" class="form-label"><?= t('user.profile.name') ?></label>
                        <input type="text" class="form-control" id="name" name="name" value="<?= htmlspecialchars($user->getName()) ?>" required>
                    </div>
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= t('user.profile.email') ?></label>
                        <input type="email" class="form-control" id="email" value="<?= htmlspecialchars($user->getEmail()) ?>" readonly disabled>
                        <div class="form-text"><?= t('user.profile.emailNotice') ?></div>
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('user.profile.updateProfile') ?></button>
                    </div>
                </form>
            </div>
        </div>

        <div class="card">
            <div class="card-header bg-secondary text-white">
                <h4 class="mb-0"><?= t('user.profile.changePassword.title') ?></h4>
            </div>
            <div class="card-body">
                <form action="/user/profile" method="post">
                    <div class="mb-3">
                        <label for="current_password" class="form-label"><?= t('user.profile.changePassword.current') ?></label>
                        <input type="password" class="form-control" id="current_password" name="current_password" required>
                    </div>
                    <div class="mb-3">
                        <label for="new_password" class="form-label"><?= t('user.profile.changePassword.new') ?></label>
                        <input type="password" class="form-control" id="new_password" name="new_password" required minlength="8">
                    </div>
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label"><?= t('user.profile.changePassword.confirm') ?></label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" required minlength="8">
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-secondary"><?= t('user.profile.changePassword.submit') ?></button>
                    </div>
                </form>
            </div>
        </div>

        <div class="mt-3 text-center">
            <a href="/user/dashboard" class="btn btn-outline-primary"><?= t('user.profile.backToDashboard') ?></a>
        </div>
    </div>
</div>