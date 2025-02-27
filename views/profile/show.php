<div class="row">
    <div class="col-lg-8 mx-auto">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h4 class="mb-0">My Profile</h4>
            </div>
            <div class="card-body">
                <form action="/profile" method="post">
                    <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                    
                    <div class="row mb-3">
                        <div class="col-md-6">
                            <label for="first_name" class="form-label">First Name</label>
                            <input type="text" class="form-control <?= isset($errors['first_name']) ? 'is-invalid' : '' ?>" 
                                id="first_name" name="first_name" value="<?= $first_name ?? $user->getFirstName() ?>" required>
                            <?php if (isset($errors['first_name'])): ?>
                                <div class="invalid-feedback"><?= $errors['first_name'] ?></div>
                            <?php endif; ?>
                        </div>
                        <div class="col-md-6">
                            <label for="last_name" class="form-label">Last Name</label>
                            <input type="text" class="form-control <?= isset($errors['last_name']) ? 'is-invalid' : '' ?>" 
                                id="last_name" name="last_name" value="<?= $last_name ?? $user->getLastName() ?>" required>
                            <?php if (isset($errors['last_name'])): ?>
                                <div class="invalid-feedback"><?= $errors['last_name'] ?></div>
                            <?php endif; ?>
                        </div>
                    </div>

                    <div class="mb-3">
                        <label for="email" class="form-label">Email Address</label>
                        <input type="email" class="form-control <?= isset($errors['email']) ? 'is-invalid' : '' ?>" 
                            id="email" name="email" value="<?= $email ?? $user->getEmail() ?>" required>
                        <?php if (isset($errors['email'])): ?>
                            <div class="invalid-feedback"><?= $errors['email'] ?></div>
                        <?php endif; ?>
                    </div>
                    
                    <hr class="my-4">
                    <h5>Change Password</h5>
                    <p class="text-muted">Leave these fields empty if you don't want to change your password</p>
                    
                    <div class="mb-3">
                        <label for="current_password" class="form-label">Current Password</label>
                        <input type="password" class="form-control <?= isset($errors['current_password']) ? 'is-invalid' : '' ?>" 
                            id="current_password" name="current_password">
                        <?php if (isset($errors['current_password'])): ?>
                            <div class="invalid-feedback"><?= $errors['current_password'] ?></div>
                        <?php endif; ?>
                    </div>
                    
                    <div class="mb-3">
                        <label for="new_password" class="form-label">New Password</label>
                        <input type="password" class="form-control <?= isset($errors['new_password']) ? 'is-invalid' : '' ?>" 
                            id="new_password" name="new_password">
                        <div class="form-text">Password must be at least 8 characters long</div>
                        <?php if (isset($errors['new_password'])): ?>
                            <div class="invalid-feedback"><?= $errors['new_password'] ?></div>
                        <?php endif; ?>
                    </div>
                    
                    <div class="mb-3">
                        <label for="confirm_password" class="form-label">Confirm New Password</label>
                        <input type="password" class="form-control <?= isset($errors['confirm_password']) ? 'is-invalid' : '' ?>" 
                            id="confirm_password" name="confirm_password">
                        <?php if (isset($errors['confirm_password'])): ?>
                            <div class="invalid-feedback"><?= $errors['confirm_password'] ?></div>
                        <?php endif; ?>
                    </div>
                    
                    <div class="text-center mt-4">
                        <button type="submit" class="btn btn-primary px-5">Save Changes</button>
                    </div>
                </form>
            </div>
        </div>
    </div>
</div>
