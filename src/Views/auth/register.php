<?php
/**
 * Registration Form View
 *
 * Displays a form that allows new users to create an account.
 * The form includes fields for name, email, password, and password confirmation.
 * Form data is preserved when there are validation errors.
 * 
 * @uses t() Translation function for internationalization
 * @uses $session Session object containing flash data from previous request
 */
?>
<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-success text-white">
                <h4 class="mb-0"><?= t('auth.register.title') ?></h4>
            </div>
            <div class="card-body">
                <!-- Registration form with POST submission -->
                <form action="/auth/register" method="post">
                    <!-- Name input field with old input value preserved -->
                    <div class="mb-3">
                        <label for="name" class="form-label"><?= t('auth.register.name') ?></label>
                        <input type="text" class="form-control" id="name" name="name" required
                            value="<?= htmlspecialchars($session->getFlash('old_input')['name'] ?? '') ?>">
                    </div>
                    
                    <!-- Email input field with old input value preserved -->
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= t('auth.register.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required
                            value="<?= htmlspecialchars($session->getFlash('old_input')['email'] ?? '') ?>">
                    </div>
                    
                    <!-- Password input with requirements hint -->
                    <div class="mb-3">
                        <label for="password" class="form-label"><?= t('auth.register.password') ?></label>
                        <input type="password" class="form-control" id="password" name="password" required minlength="8">
                        <div class="form-text"><?= t('auth.register.passwordRequirements') ?></div>
                    </div>
                    
                    <!-- Password confirmation field -->
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label"><?= t('auth.register.confirmPassword') ?></label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" required minlength="8">
                    </div>
                    
                    <!-- Submit button -->
                    <div class="d-grid">
                        <button type="submit" class="btn btn-success"><?= t('auth.register.submit') ?></button>
                    </div>
                </form>

                <hr>

                <!-- Login option for existing users -->
                <div class="text-center">
                    <p><?= t('auth.register.haveAccount') ?></p>
                    <a href="/auth/login" class="btn btn-outline-primary"><?= t('auth.login') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>