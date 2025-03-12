<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\ExclusionRule;
use SecretSanta\Models\User;
use SecretSanta\Models\Group;

class ExclusionRuleRepository extends DataMapper
{
    protected string $table = 'exclusion_rules';
    protected string $entityClass = ExclusionRule::class;
    protected array $columns = [
        'id',
        'group_id',
        'user_id',
        'excluded_user_id',
        'created_at'
    ];

    /**
     * Find all exclusion rules for a specific group
     */
    public function findByGroupId(int $groupId): array
    {
        return $this->findBy(['group_id' => $groupId]);
    }

    /**
     * Find all exclusion rules created by a specific user in a group
     */
    public function findByUserAndGroup(int $userId, int $groupId): array
    {
        return $this->findBy([
            'user_id' => $userId,
            'group_id' => $groupId
        ]);
    }

    /**
     * Find a specific exclusion rule
     */
    public function findByUserAndExcluded(int $userId, int $excludedUserId, int $groupId): ?ExclusionRule
    {
        $rules = $this->findBy([
            'user_id' => $userId,
            'excluded_user_id' => $excludedUserId,
            'group_id' => $groupId
        ]);

        return !empty($rules) ? $rules[0] : null;
    }

    /**
     * Load the user relationship for an exclusion rule
     */
    public function loadUser(ExclusionRule $rule): ExclusionRule
    {
        if ($rule->getUserId() === null) {
            return $rule;
        }

        $userRepository = new UserRepository();
        $user = $userRepository->find($rule->getUserId());

        return $rule->setUser($user);
    }

    /**
     * Load the excluded user relationship for an exclusion rule
     */
    public function loadExcludedUser(ExclusionRule $rule): ExclusionRule
    {
        if ($rule->getExcludedUserId() === null) {
            return $rule;
        }

        $userRepository = new UserRepository();
        $excludedUser = $userRepository->find($rule->getExcludedUserId());

        return $rule->setExcludedUser($excludedUser);
    }

    /**
     * Load the group relationship for an exclusion rule
     */
    public function loadGroup(ExclusionRule $rule): ExclusionRule
    {
        if ($rule->getGroupId() === null) {
            return $rule;
        }

        $groupRepository = new GroupRepository();
        $group = $groupRepository->find($rule->getGroupId());

        return $rule->setGroup($group);
    }

    /**
     * Load all relationships for an exclusion rule
     */
    public function loadRelationships(ExclusionRule $rule): ExclusionRule
    {
        $this->loadUser($rule);
        $this->loadExcludedUser($rule);
        $this->loadGroup($rule);

        return $rule;
    }

    /**
     * Add a new exclusion rule
     */
    public function addExclusion(int $groupId, int $userId, int $excludedUserId): ?ExclusionRule
    {
        // Check if already exists
        $existing = $this->findByUserAndExcluded($userId, $excludedUserId, $groupId);
        if ($existing) {
            return $existing;
        }

        // Can't exclude yourself
        if ($userId === $excludedUserId) {
            return null;
        }

        $rule = new ExclusionRule();
        $rule->setGroupId($groupId)
            ->setUserId($userId)
            ->setExcludedUserId($excludedUserId);

        return $this->save($rule);
    }

    /**
     * Remove an exclusion rule
     */
    public function removeExclusion(int $groupId, int $userId, int $excludedUserId): bool
    {
        $rule = $this->findByUserAndExcluded($userId, $excludedUserId, $groupId);

        if (!$rule) {
            return false;
        }

        return $this->delete($rule);
    }
}
