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
                        <input type="email" class="form-control" id="email" name="email" value="<?= htmlspecialchars($user->getEmail()) ?>" required>
                        <?php if ($user->getPendingEmail()): ?>
                            <div class="alert alert-info mt-2">
                                <?= t('user.profile.emailPendingVerification', ['email' => htmlspecialchars($user->getPendingEmail())]) ?>
                            </div>
                        <?php else: ?>
                            <div class="form-text"><?= t('user.profile.emailChangeNotice') ?></div>
                        <?php endif; ?>
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('user.profile.updateProfile') ?></button>
                    </div>
                </form>
            </div>
        </div>

        <!-- 
        * Password Change Card Section
        * 
        * This section provides a form for users to change their account password.
        * It requires the current password for verification and ensures the new 
        * password meets security requirements (minimum 8 characters).
        * The form includes validation for password confirmation.
        -->
        <div class="card">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('user.profile.changePassword.title') ?></h4>
            </div>
            <div class="card-body">
                <!-- 
                * Password Change Form
                * @action /user/profile - Form submits to the profile controller
                * @method post - Data is sent securely via POST method
                -->
                <form action="/user/profile" method="post">
                    <!-- Current password field with validation -->
                    <div class="mb-3">
                        <label for="current_password" class="form-label"><?= t('user.profile.changePassword.current') ?></label>
                        <input type="password" class="form-control" id="current_password" name="current_password" required>
                    </div>
                    
                    <!-- New password field with security constraints -->
                    <div class="mb-3">
                        <label for="new_password" class="form-label"><?= t('user.profile.changePassword.new') ?></label>
                        <!-- minlength=8 enforces minimum password security -->
                        <input type="password" class="form-control" id="new_password" name="new_password" required minlength="8">
                    </div>
                    
                    <!-- Password confirmation field for validation -->
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label"><?= t('user.profile.changePassword.confirm') ?></label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" required minlength="8">
                    </div>
                    
                    <!-- Submit button for password change form -->
                    <div class="d-grid">
                        <button type="submit" class="btn btn-secondary"><?= t('user.profile.changePassword.submit') ?></button>
                    </div>
                </form>
            </div>
        </div>

        <!-- Navigation link to return to dashboard -->
        <div class="mt-3 text-center">
            <a href="/user/dashboard" class="btn btn-outline-primary"><?= t('user.profile.backToDashboard') ?></a>
        </div>
    </div>
</div>