<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\ExclusionRule;
use SecretSanta\Models\User;
use SecretSanta\Models\Group;

/**
 * Repository class for managing exclusion rules
 * 
 * Handles the database operations for exclusion rules which prevent certain
 * users from being assigned to each other in the Secret Santa process.
 */
class ExclusionRuleRepository extends DataMapper
{
    /**
     * Database table name for exclusion rules
     * @var string
     */
    protected string $table = 'exclusion_rules';

    /**
     * Entity class associated with this repository
     * @var string
     */
    protected string $entityClass = ExclusionRule::class;

    /**
     * Database columns for the exclusion rules table
     * @var array
     */
    protected array $columns = [
        'id',
        'group_id',
        'user_id',
        'excluded_user_id',
        'created_at'
    ];

    /**
     * Find all exclusion rules for a specific group
     * 
     * @param int $groupId The ID of the group to find rules for
     * @return array Array of ExclusionRule objects
     */
    public function findByGroupId(int $groupId): array
    {
        return $this->findBy(['group_id' => $groupId]);
    }

    /**
     * Find all exclusion rules created by a specific user in a group
     * 
     * @param int $userId The ID of the user who created the rules
     * @param int $groupId The ID of the group the rules belong to
     * @return array Array of ExclusionRule objects
     */
    public function findByUserAndGroup(int $userId, int $groupId): array
    {
        return $this->findBy([
            'user_id' => $userId,
            'group_id' => $groupId
        ]);
    }

    /**
     * Find a specific exclusion rule by user, excluded user, and group
     * 
     * @param int $userId The ID of the user who created the rule
     * @param int $excludedUserId The ID of the user who is excluded
     * @param int $groupId The ID of the group the rule belongs to
     * @return ExclusionRule|null Returns the rule or null if not found
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
     * 
     * @param ExclusionRule $rule The exclusion rule to load the user for
     * @return ExclusionRule Returns the updated rule with user relationship loaded
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
     * 
     * @param ExclusionRule $rule The exclusion rule to load the excluded user for
     * @return ExclusionRule Returns the updated rule with excluded user relationship loaded
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
     * 
     * @param ExclusionRule $rule The exclusion rule to load the group for
     * @return ExclusionRule Returns the updated rule with group relationship loaded
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
     * 
     * @param ExclusionRule $rule The exclusion rule to load relationships for
     * @return ExclusionRule Returns the updated rule with all relationships loaded
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
     * 
     * @param int $groupId The ID of the group the rule belongs to
     * @param int $userId The ID of the user creating the rule
     * @param int $excludedUserId The ID of the user being excluded
     * @return ExclusionRule|null Returns the new rule or null if invalid (e.g., self-exclusion)
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
     * 
     * @param int $groupId The ID of the group the rule belongs to
     * @param int $userId The ID of the user who created the rule
     * @param int $excludedUserId The ID of the user who is excluded
     * @return bool True if successfully removed, false if rule not found
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
